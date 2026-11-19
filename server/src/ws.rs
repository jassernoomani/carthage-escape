//! `GET /ws?token=...`: upgrades to a WebSocket and bridges it to the room actor.
//!
//! The upgrade always succeeds; an invalid token or unknown room is reported *inside* the socket
//! (an `error` message, then close code 4000). Browsers can't read HTTP errors on a failed
//! upgrade, and this lets the client tell "stop retrying" apart from a network blip.

use std::time::Duration;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{
    AppState,
    auth::TokenKind,
    game::{
        actor::{CONNECTION_BUFFER, Outbound},
        protocol::{ClientMsg, FATAL_CLOSE_CODE, ServerEnvelope, ServerMsg, WsErrorCode},
        room::{Input, Who},
    },
};

/// Clients send a heartbeat every 10 s; silence for this long means the connection is dead.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
) -> Response {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state, query.token))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, token: String) {
    let Some(claims) = state.tokens.verify(&token) else {
        return reject(
            &mut socket,
            WsErrorCode::InvalidToken,
            "Invalid or expired token",
        )
        .await;
    };
    let Some(room) = state.rooms.get_session(&claims.code, claims.sid) else {
        return reject(
            &mut socket,
            WsErrorCode::RoomNotFound,
            "This room is no longer open",
        )
        .await;
    };
    let who = match claims.kind {
        TokenKind::Player => Who::Player(claims.sub),
        TokenKind::Host => Who::Host,
    };

    let (tx, mut rx) = mpsc::channel::<Outbound>(CONNECTION_BUFFER);
    let Some(conn) = room.connect(who, tx).await else {
        return reject(
            &mut socket,
            WsErrorCode::RoomNotFound,
            "This room is no longer open",
        )
        .await;
    };

    let (mut sink, mut stream) = socket.split();

    let mut writer = tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                Outbound::Text(text) => {
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Outbound::Close => {
                    let _ = sink.send(close_frame()).await;
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            // Writer finished: the room closed us, or the socket broke.
            _ = &mut writer => break,
            next = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => {
                match next {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        let input = match serde_json::from_str::<ClientMsg>(&text) {
                            Ok(msg) => Input::Client { conn, msg },
                            Err(_) => Input::Invalid { conn },
                        };
                        room.send(input).await;
                    }
                    Ok(Some(Ok(Message::Binary(_)))) => room.send(Input::Invalid { conn }).await,
                    Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {}
                    // Closed, errored, or idle for too long.
                    Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
                }
            }
        }
    }

    room.send(Input::Disconnect { conn }).await;
    writer.abort();
}

async fn reject(socket: &mut WebSocket, code: WsErrorCode, message: &str) {
    let envelope = ServerEnvelope {
        seq: 0,
        msg: ServerMsg::Error {
            code,
            message: message.to_owned(),
        },
    };
    if let Ok(text) = serde_json::to_string(&envelope) {
        let _ = socket.send(Message::Text(text.into())).await;
    }
    let _ = socket.send(close_frame()).await;
}

fn close_frame() -> Message {
    Message::Close(Some(CloseFrame {
        code: FATAL_CLOSE_CODE,
        reason: "".into(),
    }))
}
