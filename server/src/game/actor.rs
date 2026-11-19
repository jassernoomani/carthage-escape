//! The actor shell around the pure [`Room`]: owns the per-connection channels and the DB pool,
//! turns commands into `Input`s and carries out the resulting `Effect`s.

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::room::{ConnId, Effect, Input, JoinError, Room, Who};
use crate::db;

/// Messages queued for one WebSocket connection's writer task.
#[derive(Debug)]
pub enum Outbound {
    Text(String),
    Close,
}

/// Per-connection queue size. A client this far behind is dropped rather than slowing the room.
pub const CONNECTION_BUFFER: usize = 64;

const TICK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct JoinedPlayer {
    pub player_id: Uuid,
    pub role_index: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum JoinFailure {
    #[error(transparent)]
    Rejected(#[from] JoinError),
    #[error("could not save player: {0}")]
    Database(#[from] sqlx::Error),
    #[error("room is no longer running")]
    RoomGone,
}

pub enum RoomCommand {
    Join {
        nickname: String,
        reply: oneshot::Sender<Result<JoinedPlayer, JoinFailure>>,
    },
    Connect {
        who: Who,
        tx: mpsc::Sender<Outbound>,
        reply: oneshot::Sender<ConnId>,
    },
    Input(Input),
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct Shell {
    room: Room,
    db: PgPool,
    conns: HashMap<ConnId, mpsc::Sender<Outbound>>,
    next_conn: ConnId,
}

pub async fn run(room: Room, db: PgPool, mut rx: mpsc::Receiver<RoomCommand>) {
    let code = room.meta().code.clone();
    tracing::info!(%code, "room actor started");
    let mut shell = Shell {
        room,
        db,
        conns: HashMap::new(),
        next_conn: 1,
    };
    let mut tick = tokio::time::interval(TICK_INTERVAL);
    tick.tick().await; // the first tick fires immediately; skip it

    loop {
        let stop = tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(cmd) => shell.handle(cmd).await,
                None => true, // every handle dropped
            },
            _ = tick.tick() => shell.process(Input::Tick).await,
        };
        if stop {
            break;
        }
    }
    tracing::info!(%code, "room actor stopped");
}

impl Shell {
    /// Returns true when the actor should stop.
    async fn handle(&mut self, cmd: RoomCommand) -> bool {
        match cmd {
            RoomCommand::Join { nickname, reply } => {
                let (result, stop) = match self.join(nickname).await {
                    Ok((joined, stop)) => (Ok(joined), stop),
                    Err(e) => (Err(e), false),
                };
                let _ = reply.send(result);
                stop
            }
            RoomCommand::Connect { who, tx, reply } => {
                let conn = self.next_conn;
                self.next_conn += 1;
                self.conns.insert(conn, tx);
                let _ = reply.send(conn);
                self.process(Input::Connect { conn, who }).await
            }
            RoomCommand::Input(input) => self.process(input).await,
        }
    }

    async fn join(&mut self, nickname: String) -> Result<(JoinedPlayer, bool), JoinFailure> {
        let role_index = self.room.check_join(&nickname)?;
        // The actor handles one command at a time, so nobody can take this role meanwhile.
        let player_id =
            db::insert_player(&self.db, self.room.meta().session_id, &nickname, role_index).await?;
        let stop = self
            .process(Input::PlayerJoined {
                id: player_id,
                nickname,
                role_index,
            })
            .await;
        Ok((
            JoinedPlayer {
                player_id,
                role_index,
            },
            stop,
        ))
    }

    /// Applies an input and executes its effects. Inputs generated while executing (a
    /// connection found dead) are processed in the same loop. Returns true on `Effect::Stop`.
    async fn process(&mut self, input: Input) -> bool {
        let mut queue = VecDeque::from([input]);
        let mut stop = false;
        while let Some(input) = queue.pop_front() {
            for effect in self.room.apply(input, now_ms()) {
                match effect {
                    Effect::Send { conn, msg } => {
                        let Some(tx) = self.conns.get(&conn) else {
                            continue;
                        };
                        let text = match serde_json::to_string(&msg) {
                            Ok(t) => t,
                            Err(err) => {
                                tracing::error!(%err, "serializing server message");
                                continue;
                            }
                        };
                        if tx.try_send(Outbound::Text(text)).is_err() {
                            // Writer gone or hopelessly behind: treat as disconnected.
                            if let Some(tx) = self.conns.remove(&conn) {
                                let _ = tx.try_send(Outbound::Close);
                            }
                            queue.push_back(Input::Disconnect { conn });
                        }
                    }
                    Effect::Close { conn } => {
                        if let Some(tx) = self.conns.remove(&conn) {
                            let _ = tx.try_send(Outbound::Close);
                        }
                    }
                    Effect::StatusChanged { status, at_ms } => {
                        let session_id = self.room.meta().session_id;
                        if let Err(err) =
                            db::set_session_status(&self.db, session_id, status, at_ms).await
                        {
                            tracing::error!(%err, %session_id, ?status, "saving session status");
                        }
                    }
                    Effect::Stop => stop = true,
                }
            }
        }
        stop
    }
}
