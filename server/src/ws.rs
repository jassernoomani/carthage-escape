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
use tokio::{sync::mpsc, time::Instant};

use crate::{
    AppState,
    auth::TokenKind,
    game::{
        actor::{CONNECTION_BUFFER, Outbound},
        protocol::{ClientMsg, FATAL_CLOSE_CODE, ServerEnvelope, ServerMsg, WsErrorCode},
        room::{Input, Who},
    },
};

/// The server pings this often. Browsers answer pings automatically, even in background tabs
/// whose JavaScript timers (and so the client heartbeat) are throttled.
const PING_INTERVAL: Duration = Duration::from_secs(15);
/// No frame at all (message, heartbeat or pong) for this long means the connection is dead.
const IDLE_TIMEOUT: Duration = Duration::from_secs(40);
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
/// Flood guard: more client messages than this per second drops the connection.
const MAX_MESSAGES_PER_SEC: u32 = 20;

#[derive(Deserialize)]
pub struct WsQuery {
    #[serde(default)]
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

    // Writer: forwards the room's messages and sends periodic pings. Ends when the room drops
    // our sender, asks us to close, or the socket breaks.
    let mut writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(PING_INTERVAL);
        ping.tick().await;
        loop {
            tokio::select! {
                out = rx.recv() => match out {
                    Some(Outbound::Text(text)) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Outbound::Close) => {
                        let _ = sink.send(close_frame()).await;
                        break;
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut window_start = Instant::now();
    let mut window_count = 0u32;

    loop {
        tokio::select! {
            // Writer finished: the room closed us, or the socket broke.
            _ = &mut writer => break,
            next = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => {
                let msg = match next {
                    Ok(Some(Ok(msg))) => msg,
                    // Socket error, closed, or idle for too long.
                    Ok(Some(Err(_)) | None) | Err(_) => break,
                };
                let is_data = match msg {
                    Message::Text(_) | Message::Binary(_) => true,
                    Message::Ping(_) | Message::Pong(_) => false,
                    Message::Close(_) => break,
                };
                if is_data {
                    if window_start.elapsed() >= Duration::from_secs(1) {
                        window_start = Instant::now();
                        window_count = 0;
                    }
                    window_count += 1;
                    if window_count > MAX_MESSAGES_PER_SEC {
                        tracing::warn!(%conn, "dropping connection: message flood");
                        break;
                    }
                }
                let input = match msg {
                    Message::Text(text) => match serde_json::from_str::<ClientMsg>(&text) {
                        Ok(msg) => Input::Client { conn, msg },
                        Err(_) => Input::Invalid { conn },
                    },
                    Message::Binary(_) => Input::Invalid { conn },
                    _ => continue,
                };
                room.send(input).await;
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
