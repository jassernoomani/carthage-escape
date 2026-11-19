//! Pure room logic: `(state, input, now) -> effects`. No I/O, no clocks, no channels; the actor
//! shell (`actor.rs`) feeds inputs in and carries effects out. That keeps this fully unit-testable.

use uuid::Uuid;

use super::protocol::{
    ClientMsg, LobbyPlayer, RoomStatus, RoomSummary, Seq, ServerEnvelope, ServerMsg, WsErrorCode,
    You,
};

/// Identifies one WebSocket connection inside a room.
pub type ConnId = u64;

/// A room with no connections at all for this long is abandoned and its actor stops.
pub const IDLE_SHUTDOWN_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Player(Uuid),
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomMeta {
    pub session_id: Uuid,
    pub code: String,
    pub title: String,
    pub players_needed: u8,
    pub time_limit_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Player {
    id: Uuid,
    nickname: String,
    role_index: u8,
    ready: bool,
    conn: Option<ConnId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Connect {
        conn: ConnId,
        who: Who,
    },
    Disconnect {
        conn: ConnId,
    },
    Client {
        conn: ConnId,
        msg: ClientMsg,
    },
    /// The connection sent something that isn't a valid `ClientMsg`.
    Invalid {
        conn: ConnId,
    },
    /// A player was admitted via `check_join` and saved to the DB.
    PlayerJoined {
        id: Uuid,
        nickname: String,
        role_index: u8,
    },
    /// The host deleted the session.
    Close,
    /// Periodic timer tick.
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Send {
        conn: ConnId,
        msg: ServerEnvelope,
    },
    /// Close this connection (after any messages already queued for it).
    Close {
        conn: ConnId,
    },
    /// Persist the new session status.
    StatusChanged {
        status: RoomStatus,
        at_ms: i64,
    },
    /// Delete this player's row (removed by the host from the lobby).
    PlayerRemoved {
        id: Uuid,
    },
    /// The room is finished; the actor should stop.
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    #[error("room is full")]
    RoomFull,
    #[error("nickname taken")]
    NicknameTaken,
    #[error("game already started")]
    GameAlreadyStarted,
}

/// Outcome of a successful `check_join`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinSlot {
    /// A new player: save them, then apply `Input::PlayerJoined`.
    New { role_index: u8 },
    /// The nickname belongs to a player who is currently disconnected: hand them their seat
    /// back. This is how a kid who closed the tab (and lost their token) gets back in.
    Reclaim { player_id: Uuid, role_index: u8 },
}

#[derive(Debug, Clone)]
pub struct Room {
    meta: RoomMeta,
    status: RoomStatus,
    players: Vec<Player>,
    host_conns: Vec<ConnId>,
    seq: Seq,
    ends_at: Option<i64>,
    /// When the last connection dropped (or creation time); `None` while someone is connected.
    empty_since: Option<i64>,
}

impl Room {
    pub fn new(meta: RoomMeta, now: i64) -> Self {
        Self {
            meta,
            status: RoomStatus::Lobby,
            players: Vec::new(),
            host_conns: Vec::new(),
            seq: 0,
            ends_at: None,
            empty_since: Some(now),
        }
    }

    pub fn meta(&self) -> &RoomMeta {
        &self.meta
    }

    pub fn status(&self) -> RoomStatus {
        self.status
    }

    pub fn player_nickname(&self, id: Uuid) -> Option<String> {
        self.players
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.nickname.clone())
    }

    /// Checks whether a player with this (already validated) nickname may join. Does not modify
    /// the room: for a new player the shell saves them to the DB first, then applies
    /// `Input::PlayerJoined`.
    pub fn check_join(&self, nickname: &str) -> Result<JoinSlot, JoinError> {
        let wanted = nickname.to_lowercase();
        if let Some(existing) = self
            .players
            .iter()
            .find(|p| p.nickname.to_lowercase() == wanted)
        {
            if existing.conn.is_some() || !self.status.is_active() {
                return Err(JoinError::NicknameTaken);
            }
            return Ok(JoinSlot::Reclaim {
                player_id: existing.id,
                role_index: existing.role_index,
            });
        }
        if self.status != RoomStatus::Lobby {
            return Err(JoinError::GameAlreadyStarted);
        }
        (0..self.meta.players_needed)
            .find(|role| !self.players.iter().any(|p| p.role_index == *role))
            .map(|role_index| JoinSlot::New { role_index })
            .ok_or(JoinError::RoomFull)
    }

    pub fn apply(&mut self, input: Input, now: i64) -> Vec<Effect> {
        let mut fx = Vec::new();
        match input {
            Input::Connect { conn, who } => self.connect(conn, who, now, &mut fx),
            Input::Disconnect { conn } => self.disconnect(conn, now, &mut fx),
            Input::Client { conn, msg } => self.client_msg(conn, msg, now, &mut fx),
            Input::Invalid { conn } => {
                self.send_error(
                    conn,
                    WsErrorCode::BadMessage,
                    "Unrecognized message",
                    &mut fx,
                );
            }
            Input::PlayerJoined {
                id,
                nickname,
                role_index,
            } => {
                self.players.push(Player {
                    id,
                    nickname,
                    role_index,
                    ready: false,
                    conn: None,
                });
                self.players.sort_by_key(|p| p.role_index);
                self.seq += 1;
                self.broadcast_lobby(None, &mut fx);
            }
            Input::Close => {
                for conn in self.all_conns() {
                    self.send_error(
                        conn,
                        WsErrorCode::RoomClosed,
                        "The host closed this room",
                        &mut fx,
                    );
                    fx.push(Effect::Close { conn });
                }
                self.status = RoomStatus::Abandoned;
                fx.push(Effect::Stop);
            }
            Input::Tick => {
                if self
                    .empty_since
                    .is_some_and(|t| now - t >= IDLE_SHUTDOWN_MS)
                {
                    if self.status.is_active() {
                        self.status = RoomStatus::Abandoned;
                        fx.push(Effect::StatusChanged {
                            status: self.status,
                            at_ms: now,
                        });
                    }
                    fx.push(Effect::Stop);
                }
            }
        }
        fx
    }

    // ----- input handlers -----

    fn connect(&mut self, conn: ConnId, who: Who, now: i64, fx: &mut Vec<Effect>) {
        let you = match who {
            Who::Host => {
                self.host_conns.push(conn);
                You::Host
            }
            Who::Player(id) => {
                let Some(player) = self.players.iter_mut().find(|p| p.id == id) else {
                    self.send_error(conn, WsErrorCode::InvalidToken, "Unknown player", fx);
                    fx.push(Effect::Close { conn });
                    return;
                };
                // Same player opened a second tab/device: the newest connection wins.
                let replaced = player.conn.replace(conn);
                let you = You::Player {
                    player_id: player.id,
                    nickname: player.nickname.clone(),
                    role_index: player.role_index,
                };
                let nickname = player.nickname.clone();
                if let Some(old) = replaced {
                    self.send_error(old, WsErrorCode::Replaced, "Opened somewhere else", fx);
                    fx.push(Effect::Close { conn: old });
                } else {
                    self.seq += 1;
                    self.broadcast_lobby(Some(conn), fx);
                    if self.status != RoomStatus::Lobby {
                        self.broadcast_except(
                            conn,
                            ServerMsg::PlayerConnection {
                                nickname,
                                connected: true,
                            },
                            fx,
                        );
                    }
                }
                you
            }
        };
        self.empty_since = None;
        let welcome = ServerMsg::Welcome {
            you,
            room: self.summary(),
            status: self.status,
            players: self.lobby_players(),
            ends_at: self.ends_at,
            server_time: now,
        };
        self.send(conn, welcome, fx);
    }

    fn disconnect(&mut self, conn: ConnId, now: i64, fx: &mut Vec<Effect>) {
        if let Some(pos) = self.host_conns.iter().position(|c| *c == conn) {
            self.host_conns.remove(pos);
        } else if let Some(player) = self.players.iter_mut().find(|p| p.conn == Some(conn)) {
            player.conn = None;
            let nickname = player.nickname.clone();
            self.seq += 1;
            self.broadcast_lobby(None, fx);
            if self.status != RoomStatus::Lobby {
                self.broadcast_except(
                    conn,
                    ServerMsg::PlayerConnection {
                        nickname,
                        connected: false,
                    },
                    fx,
                );
            }
        } else {
            // Already replaced or closed: nothing to do.
            return;
        }
        if self.all_conns().is_empty() {
            self.empty_since = Some(now);
        }
    }

    fn client_msg(&mut self, conn: ConnId, msg: ClientMsg, now: i64, fx: &mut Vec<Effect>) {
        let Some(who) = self.who(conn) else {
            return;
        };
        match msg {
            ClientMsg::Heartbeat {} => {}
            ClientMsg::Ready { ready } => {
                let Who::Player(id) = who else {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotPlayer,
                        "Only players can be ready",
                        fx,
                    );
                };
                if self.status != RoomStatus::Lobby {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotInLobby,
                        "Game already started",
                        fx,
                    );
                }
                if let Some(player) = self.players.iter_mut().find(|p| p.id == id)
                    && player.ready != ready
                {
                    player.ready = ready;
                    self.seq += 1;
                    self.broadcast_lobby(None, fx);
                }
            }
            ClientMsg::HostStart {} => {
                if who != Who::Host {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotHost,
                        "Only the host can start",
                        fx,
                    );
                }
                if self.status != RoomStatus::Lobby {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotInLobby,
                        "Game already started",
                        fx,
                    );
                }
                if self.players.len() < usize::from(self.meta.players_needed) {
                    return self.send_error(
                        conn,
                        WsErrorCode::RoomNotFull,
                        "Waiting for players",
                        fx,
                    );
                }
                if !self.players.iter().all(|p| p.ready && p.conn.is_some()) {
                    return self.send_error(
                        conn,
                        WsErrorCode::PlayersNotReady,
                        "All players must be connected and ready",
                        fx,
                    );
                }
                let ends_at = now + i64::from(self.meta.time_limit_secs) * 1000;
                self.status = RoomStatus::Running;
                self.ends_at = Some(ends_at);
                self.seq += 1;
                fx.push(Effect::StatusChanged {
                    status: RoomStatus::Running,
                    at_ms: now,
                });
                for c in self.all_conns() {
                    self.send(c, ServerMsg::GameStarted { ends_at }, fx);
                }
            }
            ClientMsg::HostRemovePlayer { role_index } => {
                if who != Who::Host {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotHost,
                        "Only the host can remove players",
                        fx,
                    );
                }
                if self.status != RoomStatus::Lobby {
                    return self.send_error(
                        conn,
                        WsErrorCode::NotInLobby,
                        "Players can only be removed before the game starts",
                        fx,
                    );
                }
                let Some(pos) = self.players.iter().position(|p| p.role_index == role_index) else {
                    return self.send_error(
                        conn,
                        WsErrorCode::PlayerNotFound,
                        "No player in that seat",
                        fx,
                    );
                };
                let removed = self.players.remove(pos);
                if let Some(their_conn) = removed.conn {
                    self.send_error(
                        their_conn,
                        WsErrorCode::Removed,
                        "The host removed you from this game",
                        fx,
                    );
                    fx.push(Effect::Close { conn: their_conn });
                }
                fx.push(Effect::PlayerRemoved { id: removed.id });
                self.seq += 1;
                self.broadcast_lobby(None, fx);
                if self.all_conns().is_empty() {
                    self.empty_since = Some(now);
                }
            }
        }
    }

    // ----- helpers -----

    fn who(&self, conn: ConnId) -> Option<Who> {
        if self.host_conns.contains(&conn) {
            return Some(Who::Host);
        }
        self.players
            .iter()
            .find(|p| p.conn == Some(conn))
            .map(|p| Who::Player(p.id))
    }

    fn all_conns(&self) -> Vec<ConnId> {
        self.players
            .iter()
            .filter_map(|p| p.conn)
            .chain(self.host_conns.iter().copied())
            .collect()
    }

    fn summary(&self) -> RoomSummary {
        RoomSummary {
            code: self.meta.code.clone(),
            title: self.meta.title.clone(),
            players_needed: self.meta.players_needed,
            time_limit_secs: self.meta.time_limit_secs,
        }
    }

    fn lobby_players(&self) -> Vec<LobbyPlayer> {
        self.players
            .iter()
            .map(|p| LobbyPlayer {
                nickname: p.nickname.clone(),
                role_index: p.role_index,
                connected: p.conn.is_some(),
                ready: p.ready,
            })
            .collect()
    }

    fn send(&self, conn: ConnId, msg: ServerMsg, fx: &mut Vec<Effect>) {
        fx.push(Effect::Send {
            conn,
            msg: ServerEnvelope { seq: self.seq, msg },
        });
    }

    fn send_error(&self, conn: ConnId, code: WsErrorCode, message: &str, fx: &mut Vec<Effect>) {
        self.send(
            conn,
            ServerMsg::Error {
                code,
                message: message.to_owned(),
            },
            fx,
        );
    }

    fn broadcast_except(&self, except: ConnId, msg: ServerMsg, fx: &mut Vec<Effect>) {
        for c in self.all_conns().into_iter().filter(|c| *c != except) {
            self.send(c, msg.clone(), fx);
        }
    }

    /// Sends the current lobby to everyone (optionally skipping one connection).
    fn broadcast_lobby(&self, except: Option<ConnId>, fx: &mut Vec<Effect>) {
        let msg = ServerMsg::LobbyUpdate {
            players: self.lobby_players(),
        };
        for c in self.all_conns().into_iter().filter(|c| Some(*c) != except) {
            self.send(c, msg.clone(), fx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: ConnId = 100;
    const A: ConnId = 1;
    const B: ConnId = 2;

    fn meta() -> RoomMeta {
        RoomMeta {
            session_id: Uuid::nil(),
            code: "ABC234".into(),
            title: "Test Room".into(),
            players_needed: 2,
            time_limit_secs: 600,
        }
    }

    /// Room with two joined (not yet connected) players. Returns their ids.
    fn room_with_two() -> (Room, Uuid, Uuid) {
        let mut room = Room::new(meta(), 0);
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        for (id, name) in [(a, "Ali"), (b, "Bea")] {
            let Ok(JoinSlot::New { role_index: role }) = room.check_join(name) else {
                panic!("{name} should be able to join");
            };
            room.apply(
                Input::PlayerJoined {
                    id,
                    nickname: name.into(),
                    role_index: role,
                },
                0,
            );
        }
        (room, a, b)
    }

    /// Two connected players plus a connected host.
    fn full_room() -> (Room, Uuid, Uuid) {
        let (mut room, a, b) = room_with_two();
        room.apply(
            Input::Connect {
                conn: HOST,
                who: Who::Host,
            },
            1,
        );
        room.apply(
            Input::Connect {
                conn: A,
                who: Who::Player(a),
            },
            1,
        );
        room.apply(
            Input::Connect {
                conn: B,
                who: Who::Player(b),
            },
            1,
        );
        (room, a, b)
    }

    fn sent_to(fx: &[Effect], conn: ConnId) -> Vec<&ServerMsg> {
        fx.iter()
            .filter_map(|e| match e {
                Effect::Send { conn: c, msg } if *c == conn => Some(&msg.msg),
                _ => None,
            })
            .collect()
    }

    fn error_code(fx: &[Effect], conn: ConnId) -> Option<WsErrorCode> {
        sent_to(fx, conn).into_iter().find_map(|m| match m {
            ServerMsg::Error { code, .. } => Some(*code),
            _ => None,
        })
    }

    fn ready(room: &mut Room, conn: ConnId) -> Vec<Effect> {
        room.apply(
            Input::Client {
                conn,
                msg: ClientMsg::Ready { ready: true },
            },
            2,
        )
    }

    fn start(room: &mut Room, conn: ConnId) -> Vec<Effect> {
        room.apply(
            Input::Client {
                conn,
                msg: ClientMsg::HostStart {},
            },
            1_000,
        )
    }

    #[test]
    fn join_assigns_roles_and_enforces_capacity_and_unique_names() {
        let (room, _, _) = room_with_two();
        assert_eq!(room.check_join("Cyd"), Err(JoinError::RoomFull));

        let mut room = Room::new(meta(), 0);
        assert_eq!(room.check_join("Ali"), Ok(JoinSlot::New { role_index: 0 }));
        let ali = Uuid::new_v4();
        room.apply(
            Input::PlayerJoined {
                id: ali,
                nickname: "Ali".into(),
                role_index: 0,
            },
            0,
        );
        assert_eq!(room.check_join("Bea"), Ok(JoinSlot::New { role_index: 1 }));

        // While Ali is connected, the name is taken (case-insensitive).
        room.apply(
            Input::Connect {
                conn: A,
                who: Who::Player(ali),
            },
            1,
        );
        assert_eq!(room.check_join("ALI"), Err(JoinError::NicknameTaken));
    }

    #[test]
    fn disconnected_player_can_reclaim_their_seat_by_nickname() {
        let (mut room, a, _) = full_room();
        ready(&mut room, A);
        ready(&mut room, B);
        start(&mut room, HOST);

        // Ali closes the tab (token lost). Typing the same name gives the same seat back,
        // even though the game has started and the room is full.
        room.apply(Input::Disconnect { conn: A }, 2_000);
        assert_eq!(
            room.check_join("ali"),
            Ok(JoinSlot::Reclaim {
                player_id: a,
                role_index: 0
            })
        );
        // New names are still refused once the game is running.
        assert_eq!(room.check_join("Cyd"), Err(JoinError::GameAlreadyStarted));
    }

    #[test]
    fn host_removes_player_from_lobby() {
        let (mut room, a, _) = full_room();
        let remove = |room: &mut Room, conn, role_index| {
            room.apply(
                Input::Client {
                    conn,
                    msg: ClientMsg::HostRemovePlayer { role_index },
                },
                5,
            )
        };

        assert_eq!(
            error_code(&remove(&mut room, B, 0), B),
            Some(WsErrorCode::NotHost)
        );
        assert_eq!(
            error_code(&remove(&mut room, HOST, 9), HOST),
            Some(WsErrorCode::PlayerNotFound)
        );

        let fx = remove(&mut room, HOST, 0);
        assert_eq!(error_code(&fx, A), Some(WsErrorCode::Removed));
        assert!(fx.contains(&Effect::Close { conn: A }));
        assert!(fx.contains(&Effect::PlayerRemoved { id: a }));
        let lobby = sent_to(&fx, B);
        assert!(
            matches!(lobby.as_slice(), [ServerMsg::LobbyUpdate { players }] if players.len() == 1)
        );

        // The seat is free again, and the old token no longer works.
        assert_eq!(room.check_join("Cyd"), Ok(JoinSlot::New { role_index: 0 }));
        let fx = room.apply(
            Input::Connect {
                conn: 9,
                who: Who::Player(a),
            },
            6,
        );
        assert_eq!(error_code(&fx, 9), Some(WsErrorCode::InvalidToken));
    }

    #[test]
    fn players_cannot_be_removed_once_the_game_runs() {
        let (mut room, _, _) = full_room();
        ready(&mut room, A);
        ready(&mut room, B);
        start(&mut room, HOST);
        let fx = room.apply(
            Input::Client {
                conn: HOST,
                msg: ClientMsg::HostRemovePlayer { role_index: 0 },
            },
            5,
        );
        assert_eq!(error_code(&fx, HOST), Some(WsErrorCode::NotInLobby));
    }

    #[test]
    fn connect_sends_welcome_snapshot_and_notifies_others() {
        let (mut room, a, _) = room_with_two();
        room.apply(
            Input::Connect {
                conn: HOST,
                who: Who::Host,
            },
            1,
        );
        let fx = room.apply(
            Input::Connect {
                conn: A,
                who: Who::Player(a),
            },
            5,
        );

        let to_a = sent_to(&fx, A);
        let [
            ServerMsg::Welcome {
                you,
                players,
                status,
                server_time,
                ..
            },
        ] = to_a.as_slice()
        else {
            panic!("expected exactly a welcome, got {to_a:?}");
        };
        assert!(matches!(you, You::Player { nickname, role_index: 0, .. } if nickname == "Ali"));
        assert_eq!(*status, RoomStatus::Lobby);
        assert_eq!(*server_time, 5);
        assert!(players[0].connected && !players[1].connected);

        let to_host = sent_to(&fx, HOST);
        assert!(
            matches!(to_host.as_slice(), [ServerMsg::LobbyUpdate { players }] if players[0].connected)
        );
    }

    #[test]
    fn unknown_player_is_rejected() {
        let (mut room, _, _) = room_with_two();
        let fx = room.apply(
            Input::Connect {
                conn: 9,
                who: Who::Player(Uuid::new_v4()),
            },
            1,
        );
        assert_eq!(error_code(&fx, 9), Some(WsErrorCode::InvalidToken));
        assert!(fx.contains(&Effect::Close { conn: 9 }));
    }

    #[test]
    fn newer_connection_replaces_older_one() {
        let (mut room, a, _) = full_room();
        let fx = room.apply(
            Input::Connect {
                conn: 7,
                who: Who::Player(a),
            },
            2,
        );
        assert_eq!(error_code(&fx, A), Some(WsErrorCode::Replaced));
        assert!(fx.contains(&Effect::Close { conn: A }));
        assert!(matches!(
            sent_to(&fx, 7).as_slice(),
            [ServerMsg::Welcome { .. }]
        ));

        // The late Disconnect for the old connection must not mark the player offline.
        let fx = room.apply(Input::Disconnect { conn: A }, 3);
        assert!(fx.is_empty());
        assert!(room.lobby_players()[0].connected);
    }

    #[test]
    fn ready_toggles_and_broadcasts() {
        let (mut room, _, _) = full_room();
        let seq_before = room.seq;
        let fx = ready(&mut room, A);
        assert!(room.lobby_players()[0].ready);
        assert_eq!(room.seq, seq_before + 1);
        for conn in [A, B, HOST] {
            assert!(matches!(
                sent_to(&fx, conn).as_slice(),
                [ServerMsg::LobbyUpdate { .. }]
            ));
        }
        // Setting the same value again is a no-op.
        assert!(ready(&mut room, A).is_empty());
    }

    #[test]
    fn host_cannot_be_ready_and_player_cannot_start() {
        let (mut room, _, _) = full_room();
        assert_eq!(
            error_code(&ready(&mut room, HOST), HOST),
            Some(WsErrorCode::NotPlayer)
        );
        assert_eq!(
            error_code(&start(&mut room, A), A),
            Some(WsErrorCode::NotHost)
        );
    }

    #[test]
    fn start_requires_full_connected_ready_room() {
        let mut room = Room::new(meta(), 0);
        room.apply(
            Input::Connect {
                conn: HOST,
                who: Who::Host,
            },
            0,
        );
        assert_eq!(
            error_code(&start(&mut room, HOST), HOST),
            Some(WsErrorCode::RoomNotFull)
        );

        let (mut room, _, _) = full_room();
        ready(&mut room, A);
        assert_eq!(
            error_code(&start(&mut room, HOST), HOST),
            Some(WsErrorCode::PlayersNotReady)
        );

        ready(&mut room, B);
        room.apply(Input::Disconnect { conn: B }, 3);
        assert_eq!(
            error_code(&start(&mut room, HOST), HOST),
            Some(WsErrorCode::PlayersNotReady),
            "a ready but disconnected player blocks the start"
        );
    }

    #[test]
    fn start_runs_game_for_everyone() {
        let (mut room, _, _) = full_room();
        ready(&mut room, A);
        ready(&mut room, B);
        let fx = start(&mut room, HOST);

        assert_eq!(room.status(), RoomStatus::Running);
        assert!(fx.contains(&Effect::StatusChanged {
            status: RoomStatus::Running,
            at_ms: 1_000
        }));
        let expected_end = 1_000 + 600 * 1000;
        for conn in [A, B, HOST] {
            assert_eq!(
                sent_to(&fx, conn),
                vec![&ServerMsg::GameStarted {
                    ends_at: expected_end
                }]
            );
        }
        // Once started: no more joining, readying, or starting again.
        assert_eq!(room.check_join("Cyd"), Err(JoinError::GameAlreadyStarted));
        assert_eq!(
            error_code(&start(&mut room, HOST), HOST),
            Some(WsErrorCode::NotInLobby)
        );
    }

    #[test]
    fn disconnect_during_game_notifies_partner() {
        let (mut room, _, _) = full_room();
        ready(&mut room, A);
        ready(&mut room, B);
        start(&mut room, HOST);
        let fx = room.apply(Input::Disconnect { conn: A }, 2_000);
        assert!(sent_to(&fx, B).contains(&&ServerMsg::PlayerConnection {
            nickname: "Ali".into(),
            connected: false
        }));
    }

    #[test]
    fn reconnect_gets_running_snapshot() {
        let (mut room, a, _) = full_room();
        ready(&mut room, A);
        ready(&mut room, B);
        start(&mut room, HOST);
        room.apply(Input::Disconnect { conn: A }, 2_000);
        let fx = room.apply(
            Input::Connect {
                conn: 8,
                who: Who::Player(a),
            },
            3_000,
        );
        let to_new = sent_to(&fx, 8);
        assert!(matches!(
            to_new.as_slice(),
            [ServerMsg::Welcome {
                status: RoomStatus::Running,
                ends_at: Some(_),
                ..
            }]
        ));
    }

    #[test]
    fn seq_is_monotonic_across_changes() {
        let (mut room, _, _) = full_room();
        let mut last = room.seq;
        let steps: [fn(&mut Room) -> Vec<Effect>; 3] =
            [|r| ready(r, A), |r| ready(r, B), |r| start(r, HOST)];
        for step in steps {
            let fx = step(&mut room);
            for e in &fx {
                if let Effect::Send { msg, .. } = e {
                    assert!(msg.seq > last, "seq must grow on each state change");
                }
            }
            last = room.seq;
        }
    }

    #[test]
    fn invalid_message_gets_error_and_connection_survives() {
        let (mut room, _, _) = full_room();
        let fx = room.apply(Input::Invalid { conn: A }, 2);
        assert_eq!(error_code(&fx, A), Some(WsErrorCode::BadMessage));
        assert!(!fx.contains(&Effect::Close { conn: A }));
    }

    #[test]
    fn close_kicks_everyone_and_stops() {
        let (mut room, _, _) = full_room();
        let fx = room.apply(Input::Close, 5);
        for conn in [A, B, HOST] {
            assert_eq!(error_code(&fx, conn), Some(WsErrorCode::RoomClosed));
            assert!(fx.contains(&Effect::Close { conn }));
        }
        assert_eq!(fx.last(), Some(&Effect::Stop));
    }

    #[test]
    fn idle_room_is_abandoned_only_after_timeout_with_nobody_connected() {
        let mut room = Room::new(meta(), 0);
        assert!(room.apply(Input::Tick, IDLE_SHUTDOWN_MS - 1).is_empty());

        room.apply(
            Input::Connect {
                conn: HOST,
                who: Who::Host,
            },
            10,
        );
        assert!(
            room.apply(Input::Tick, IDLE_SHUTDOWN_MS * 3).is_empty(),
            "host connected"
        );

        room.apply(Input::Disconnect { conn: HOST }, 1_000);
        assert!(
            room.apply(Input::Tick, 1_000 + IDLE_SHUTDOWN_MS - 1)
                .is_empty()
        );
        let fx = room.apply(Input::Tick, 1_000 + IDLE_SHUTDOWN_MS);
        assert_eq!(
            fx,
            vec![
                Effect::StatusChanged {
                    status: RoomStatus::Abandoned,
                    at_ms: 1_000 + IDLE_SHUTDOWN_MS
                },
                Effect::Stop
            ]
        );
    }
}
