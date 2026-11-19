use std::sync::Arc;

use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::{
    actor::{self, JoinFailure, JoinedPlayer, Outbound, RoomCommand},
    room::{ConnId, Input, Room, RoomMeta, Who},
};

const COMMAND_BUFFER: usize = 256;

/// Maps active room codes to their actor handles.
#[derive(Clone, Default)]
pub struct RoomManager {
    rooms: Arc<DashMap<String, RoomHandle>>,
}

/// A cheap, cloneable way to talk to one room actor.
#[derive(Clone)]
pub struct RoomHandle {
    pub session_id: Uuid,
    tx: mpsc::Sender<RoomCommand>,
}

impl RoomManager {
    /// Starts an actor for a freshly created session. The actor removes itself when it stops.
    pub fn spawn(&self, meta: RoomMeta, db: PgPool) -> RoomHandle {
        let (tx, rx) = mpsc::channel(COMMAND_BUFFER);
        let session_id = meta.session_id;
        let code = meta.code.clone();
        let handle = RoomHandle { session_id, tx };
        self.rooms.insert(code.clone(), handle.clone());

        let rooms = Arc::clone(&self.rooms);
        tokio::spawn(async move {
            actor::run(Room::new(meta, actor::now_ms()), db, rx).await;
            rooms.remove_if(&code, |_, h| h.session_id == session_id);
        });
        handle
    }

    pub fn get(&self, code: &str) -> Option<RoomHandle> {
        self.rooms.get(code).map(|h| h.clone())
    }

    /// The handle for `code`, but only if it belongs to `session_id` (codes get reused).
    pub fn get_session(&self, code: &str, session_id: Uuid) -> Option<RoomHandle> {
        self.get(code).filter(|h| h.session_id == session_id)
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }
}

impl RoomHandle {
    pub async fn join(&self, nickname: String) -> Result<JoinedPlayer, JoinFailure> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(RoomCommand::Join { nickname, reply })
            .await
            .map_err(|_| JoinFailure::RoomGone)?;
        rx.await.map_err(|_| JoinFailure::RoomGone)?
    }

    /// Registers a connection; `None` if the room has stopped.
    pub async fn connect(&self, who: Who, tx: mpsc::Sender<Outbound>) -> Option<ConnId> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(RoomCommand::Connect { who, tx, reply })
            .await
            .ok()?;
        rx.await.ok()
    }

    /// Fire-and-forget input. Errors mean the room already stopped, which is fine to ignore.
    pub async fn send(&self, input: Input) {
        let _ = self.tx.send(RoomCommand::Input(input)).await;
    }
}
