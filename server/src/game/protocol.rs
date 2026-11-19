//! WebSocket protocol (PROJECT_SPEC.md §7). Exported to TypeScript via ts-rs; never duplicate
//! these types by hand on the frontend.
//!
//! Every server message is wrapped in a [`ServerEnvelope`] carrying the room's `seq`, which
//! increases each time the shared room state changes. A client that sees `seq` jump by more
//! than one after reconnecting knows it missed updates (the `welcome` snapshot covers them).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub use super::puzzles::{ActionOutcome, PuzzleAction, PuzzleView};

pub type Seq = u32;

/// Mirrors the `session_status` Postgres enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "session_status", rename_all = "snake_case")]
#[ts(export)]
pub enum RoomStatus {
    Lobby,
    Running,
    Paused,
    Escaped,
    Failed,
    Abandoned,
}

impl RoomStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            RoomStatus::Lobby | RoomStatus::Running | RoomStatus::Paused
        )
    }
}

// ---------- Client → server ----------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[ts(export)]
pub enum ClientMsg {
    /// Player toggles their "I'm ready" flag in the lobby.
    Ready {
        ready: bool,
    },
    /// Host starts the game (all players present and ready).
    HostStart {},
    /// Host removes a player from the lobby (wrong name, left for good), freeing their slot.
    HostRemovePlayer {
        role_index: u8,
    },
    /// A player acts on the current puzzle. `puzzle_index` guards against stale clicks.
    PuzzleAction {
        puzzle_index: u32,
        action: PuzzleAction,
    },
    Heartbeat {},
}

// ---------- Server → client ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ServerEnvelope {
    pub seq: Seq,
    #[serde(flatten)]
    #[ts(flatten)]
    pub msg: ServerMsg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMsg {
    /// Full snapshot, sent on every (re)connect.
    Welcome {
        you: You,
        room: RoomSummary,
        status: RoomStatus,
        players: Vec<LobbyPlayer>,
        /// Unix time in ms when the timer runs out, once the game has started.
        #[ts(type = "number | null")]
        ends_at: Option<i64>,
        /// Server clock (Unix ms) so clients can correct for clock skew.
        #[ts(type = "number")]
        server_time: i64,
        /// Where the team is in the puzzle chain, once the game has started.
        progress: Option<PuzzleProgress>,
        /// This player's view of the current puzzle (`None` for the host and before the start).
        view: Option<PuzzleView>,
        /// Set once the room is over.
        completed: Option<Completion>,
    },
    LobbyUpdate {
        players: Vec<LobbyPlayer>,
    },
    GameStarted {
        #[ts(type = "number")]
        ends_at: i64,
        /// The chain's length is known from the start (the host never gets puzzle views).
        progress: PuzzleProgress,
    },
    /// This player's (new) view of a puzzle.
    ViewUpdate {
        progress: PuzzleProgress,
        view: PuzzleView,
    },
    /// Someone acted on the puzzle; everyone hears how it went (never the answer).
    ActionResult {
        puzzle_index: u32,
        outcome: ActionOutcome,
        by: String,
    },
    PuzzleSolved {
        puzzle_index: u32,
        next_index: Option<u32>,
    },
    RoomCompleted(Completion),
    PlayerConnection {
        nickname: String,
        connected: bool,
    },
    Error {
        code: WsErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum You {
    Player {
        player_id: Uuid,
        nickname: String,
        role_index: u8,
    },
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PuzzleProgress {
    /// 0-based index of the current puzzle.
    pub index: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Completion {
    pub escaped: bool,
    /// Seconds from the start to the end of the game.
    pub time_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RoomSummary {
    pub code: String,
    pub title: String,
    pub players_needed: u8,
    pub time_limit_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LobbyPlayer {
    pub nickname: String,
    pub role_index: u8,
    pub connected: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WsErrorCode {
    // Recoverable: the connection stays open.
    BadMessage,
    NotHost,
    NotPlayer,
    NotInLobby,
    RoomNotFull,
    PlayersNotReady,
    PlayerNotFound,
    /// The game isn't running (not started yet, or already over).
    NotRunning,
    /// The action targets a puzzle that is no longer the current one.
    StalePuzzle,
    /// The puzzle refused the action (wrong role, malformed input...).
    InvalidAction,
    // Fatal: the server closes the socket right after (close code 4000). Don't reconnect.
    InvalidToken,
    RoomNotFound,
    Replaced,
    RoomClosed,
    Removed,
}

impl WsErrorCode {
    pub fn is_fatal(self) -> bool {
        matches!(
            self,
            WsErrorCode::InvalidToken
                | WsErrorCode::RoomNotFound
                | WsErrorCode::Replaced
                | WsErrorCode::RoomClosed
                | WsErrorCode::Removed
        )
    }
}

/// WebSocket close code sent after a fatal error. Clients must not reconnect after it.
pub const FATAL_CLOSE_CODE: u16 = 4000;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn client_messages_use_type_data_shape() {
        let ready: ClientMsg =
            serde_json::from_value(json!({"type": "ready", "data": {"ready": true}})).unwrap();
        assert_eq!(ready, ClientMsg::Ready { ready: true });
        let start: ClientMsg =
            serde_json::from_value(json!({"type": "host_start", "data": {}})).unwrap();
        assert_eq!(start, ClientMsg::HostStart {});
        assert!(serde_json::from_value::<ClientMsg>(json!({"type": "nope", "data": {}})).is_err());
    }

    #[test]
    fn client_messages_roundtrip() {
        for msg in [
            ClientMsg::Ready { ready: false },
            ClientMsg::HostStart {},
            ClientMsg::HostRemovePlayer { role_index: 1 },
            ClientMsg::Heartbeat {},
        ] {
            let text = serde_json::to_string(&msg).unwrap();
            assert_eq!(serde_json::from_str::<ClientMsg>(&text).unwrap(), msg);
        }
    }

    #[test]
    fn envelope_flattens_seq_next_to_type() {
        let env = ServerEnvelope {
            seq: 7,
            msg: ServerMsg::GameStarted {
                ends_at: 1000,
                progress: PuzzleProgress { index: 0, total: 5 },
            },
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({"seq": 7, "type": "game_started", "data": {"ends_at": 1000, "progress": {"index": 0, "total": 5}}})
        );
    }
}
