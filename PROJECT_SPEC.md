# Carthage Escape: a co-op educational escape room for kids

> Working title. A real-time, multiplayer, web-based escape room for children aged 8–12, in which **no single player can solve a puzzle alone**. Each player sees different clues, and the team only escapes by communicating and cooperating.
>
> This document is the project's source of truth. It describes the product, the architecture, the protocol, the data model and the build roadmap. Read it fully before starting a milestone.

---

## 1. Product overview

### 1.1 The idea
- 2 players (3–4 later) join a **room** from separate devices (tablets or laptops).
- The room is themed (first room: **ancient Carthage**) and consists of a **chain of puzzles**.
- Every puzzle is **asymmetric**: the information needed to solve it is split across players. Player A might see a lock while Player B holds the manual that explains how to open it.
- The team wins by solving every puzzle before the timer runs out. Hints are available but cost time.

### 1.2 Educational value
| Skill | How it's practised |
|---|---|
| Math | sums, multiplication, fractions, balancing, number patterns |
| Logic | deduction, sequences, conditional rules |
| Reading comprehension | following written instructions from a "manual" |
| Languages | symbol/word translation puzzles in Arabic, French and English |
| Orientation | giving and following directions (north/south, left/right) |
| Teamwork & communication | the core mechanic: describing, listening, combining clues |

### 1.3 Target users
- **Kids (players):** join with a room code and a nickname. They have no account and give no personal data.
- **Teachers / parents (hosts):** have accounts, create rooms, watch progress live and read reports afterwards.

### 1.4 Kid-safety principles (non-negotiable)
- **No free-text chat** between players. Communication goes through a **communication wheel** (colors, numbers, arrows, symbols, pings) or, in a classroom, by talking out loud.
- **No personal data from kids:** a nickname only, filtered against a blocklist and limited to 16 characters.
- **No public matchmaking.** Players can only join a room through a code created by a host.
- **No ads, no tracking, no third-party analytics.**
- Session data is linked to the host's account and can be deleted by the host.

---

## 2. Scope

### 2.1 MVP (v1): must ship
- A host account (email + password), and creating a game session that produces a 6-character room code.
- 2 players join with the code + a nickname and land in a lobby.
- **1 themed room (Carthage) with 5 puzzles**, a timer and hints.
- Each player sees only their own view. The server is authoritative and validates every answer.
- The communication wheel (preset signals).
- Reconnection: a player who drops can rejoin and continue.
- A live host dashboard (current puzzle, time per puzzle, hints used) plus an end-of-session report.
- English UI, with an i18n structure ready for French and Arabic (RTL).
- Dockerized and deployed on a VPS with HTTPS and CI.

### 2.2 Post-MVP (stretch)
- French and Arabic (RTL) translations.
- Rooms for 3–4 players.
- More themes (Egyptian tomb, space station).
- A puzzle editor for teachers (they create their own rooms from templates).
- Procedural room generation (random puzzle chains matched to a difficulty level).
- Session replays from the event log.
- Classes (a host manages a group of kids across sessions) and progress over time.

### 2.3 Out of scope
- Native mobile apps (responsive web only).
- Voice or video chat.
- Accounts for children.

---

## 3. Tech stack

| Layer | Choice | Why |
|---|---|---|
| Backend (API + game server) | **Rust**: `axum`, `tokio`, `tokio-tungstenite` (via axum's WS support), `serde` | Authoritative real-time server, strong typing, performance |
| DB access | `sqlx` (PostgreSQL, compile-time checked queries, migrations) | |
| Auth | Argon2 password hashing (`argon2`), HTTP-only session cookie for hosts, signed player tokens (`jsonwebtoken`) for reconnection | |
| Database | **PostgreSQL 16** | Accounts, content, sessions, event log (JSONB) |
| Cache / pub-sub (later) | Redis | Only needed once there is more than one game server instance |
| Frontend | **React + TypeScript + Vite** | |
| Rendering | Plain React/CSS for most puzzles; **PixiJS** for spatial puzzles (the maze) | |
| Styling | Tailwind CSS | |
| State (client) | Zustand | |
| Shared types | `ts-rs`: generate TypeScript types from Rust structs | One source of truth for the protocol |
| i18n | `react-i18next` | Arabic RTL later |
| Testing | `cargo test` (unit + integration), Vitest (frontend), Playwright (E2E, 2 browser contexts) | |
| Ops | Docker + Docker Compose, Caddy (reverse proxy, automatic HTTPS), GitHub Actions | |

> A single Rust binary serves the REST API, the WebSocket game endpoint and (in production) the built frontend's static files. This is simpler for a solo project, and modules keep the code separated.

---

## 4. Architecture

```
 ┌──────────────┐      HTTPS (REST)      ┌────────────────────────────────────┐
 │ Host browser │ ─────────────────────► │            Rust server             │
 │ (dashboard)  │ ◄──── WebSocket ─────► │                                    │
 └──────────────┘                        │  ┌──────────┐   ┌───────────────┐  │
                                         │  │ REST API │   │ Game module   │  │
 ┌──────────────┐                        │  │ (axum)   │   │ RoomManager   │  │
 │ Player A     │ ◄──── WebSocket ─────► │  └────┬─────┘   │  └─ Room actor│  │
 │ (tablet)     │                        │       │         │     per game  │  │
 └──────────────┘                        │       │         └──────┬────────┘  │
 ┌──────────────┐                        │       ▼                ▼           │
 │ Player B     │ ◄──── WebSocket ─────► │  ┌──────────────────────────────┐  │
 │ (tablet)     │                        │  │  PostgreSQL (sqlx)           │  │
 └──────────────┘                        │  └──────────────────────────────┘  │
                                         └────────────────────────────────────┘
```

### 4.1 Key design: one actor per room
- Each live game session is a **Room actor**: a tokio task that owns the room state and receives commands through an `mpsc` channel.
- WebSocket connection tasks never touch the state directly. They send `RoomCommand`s to the actor and receive outgoing messages through a per-connection channel.
- This avoids locks around game state and makes the logic easy to test, because the room logic is a pure function: `(state, command) -> (new_state, events, outgoing messages)`.
- The `RoomManager` (an `Arc<DashMap<RoomCode, RoomHandle>>`) maps room codes to actor handles.

### 4.2 Key design: per-player state projection
- The server holds the **full** room state, including all secrets and answers.
- Players never receive the full state. For each player the server computes `view_for(player)`, a projection that contains only what that player should see.
- **Answers are never sent to clients.** All validation happens on the server.
- Hosts receive a separate `HostView` (progress and stats, no answers needed).

### 4.3 Key design: event sourcing (light)
- Every meaningful thing that happens (a player joined, an action was submitted, a puzzle was solved, a hint was used, a player disconnected) is appended to `session_events` as JSONB, with a sequence number and a timestamp.
- The host report and the analytics are computed from events. Replays (a stretch goal) also become possible.

---

## 5. Puzzle engine

Puzzles are **types implemented in code** and **instances defined as data**.

### 5.1 The Puzzle trait (Rust)
```rust
pub trait PuzzleType {
    type Params: DeserializeOwned;     // from the room definition file
    type State: Clone + Serialize;      // full server-side state (incl. answer)
    type Action: DeserializeOwned;      // what a player can submit
    type View: Serialize;               // what one player sees

    fn init(params: &Self::Params, seed: u64, roles: &[PlayerId]) -> Self::State;
    fn view_for(state: &Self::State, player: PlayerId) -> Self::View;
    fn apply(state: &mut Self::State, player: PlayerId, action: Self::Action)
        -> Result<ActionOutcome, PuzzleError>;   // Progress | Wrong | Solved
    fn hint(state: &Self::State, level: u8) -> Option<HintText>;
}
```
- `seed` makes puzzles **deterministic and reproducible**: the same seed gives the same numbers. Use a seeded RNG (`rand_chacha`).
- Roles are assigned at `init` (e.g. `roles[0]` = "lock holder", `roles[1]` = "manual reader").
- The engine wraps all puzzle types in an enum (`AnyPuzzle`) so a room can hold a heterogeneous list.

### 5.2 Room definition (data file)
Rooms live in `content/rooms/*.yaml` (and later in the DB, for the teacher editor).

```yaml
id: carthage-01
title: "The Merchant's Vault"
theme: carthage
players: 2
time_limit_secs: 1200
hint_penalty_secs: 60
puzzles:
  - type: lock_and_manual
    params: { digits: 3, rule: "sum_by_color", difficulty: easy }
  - type: blind_maze
    params: { width: 6, height: 6 }
  - type: translator
    params: { word_count: 4, language: en, symbol_set: punic }
  - type: balance_scale
    params: { weights_per_player: 3, target: "equal", allow_fractions: false }
  - type: timeline
    params: { deck: carthage_history, cards: 6 }
```

### 5.3 MVP puzzle types

| Type | Player A sees | Player B sees | Solve condition | Teaches |
|---|---|---|---|---|
| `lock_and_manual` | A lock with colored numbers and a digit input | A "manual" of rules (e.g. "code = sum of red numbers, then the blue one") | A enters the correct code | Arithmetic, reading |
| `blind_maze` | Full maze from above + B's position | Only the adjacent walls, plus movement controls | B reaches the exit | Directions, communication |
| `translator` | A door with symbols, and letter-slot inputs | The symbol-to-letter table | A types the decoded word | Vocabulary, mapping |
| `balance_scale` | Their own weights + the scale | Their own weights + the scale | Both place weights so the scale balances exactly | Addition, logic |
| `timeline` | Half of the event cards | The other half | The shared timeline is ordered correctly | History, sequencing |

Rules for every puzzle type:
- **Each type must have unit tests** for `init` (determinism with a seed), `view_for` (no leaking secrets, so assert the answer is absent from every player's view), and `apply` (right, wrong and edge cases).
- Wrong answers give feedback ("not quite!") and never reveal the solution.
- Hints come in 3 levels: a gentle nudge, a strong nudge, then a near-solution.

---

## 6. Communication wheel

- A radial menu that players open on any device.
- Signals: **colors** (6), **numbers** (0–9), **arrows** (↑↓←→), **symbols** (the current puzzle's symbol set), **reactions** (✅ ❌ ❓ 👍), and a **ping** (tap an object in your own view to highlight it for your partner, if they can also see it).
- Signals appear briefly on the partner's screen, together with a soft sound.
- Rate limit: at most 5 signals per 3 seconds per player.
- In classroom mode, players can simply talk; the wheel is still available.

---

## 7. WebSocket protocol

- Endpoint: `GET /ws?token=<player_or_host_token>`
- Messages are JSON in the shape `{ "type": "...", "data": { ... } }`, tagged enums in Rust (`#[serde(tag = "type", content = "data", rename_all = "snake_case")]`).
- TypeScript types are generated with `ts-rs` into `web/src/generated/`.
- Every server message carries a `seq` (monotonic per room) so a client that reconnects can detect gaps.

### 7.1 Client → server
| type | data | who |
|---|---|---|
| `ready` | `{}` | player |
| `puzzle_action` | `{ puzzle_index, action }` (action is specific to the puzzle type) | player |
| `signal` | `{ kind, value }` | player |
| `request_hint` | `{ puzzle_index }` | player |
| `host_start` | `{}` | host |
| `host_pause` / `host_resume` | `{}` | host |
| `heartbeat` | `{}` | all |

### 7.2 Server → client
| type | data | to |
|---|---|---|
| `welcome` | `{ you: PlayerInfo, room: RoomSummary, view, seq }` (full snapshot on connect or reconnect) | each |
| `lobby_update` | `{ players: [{ nickname, connected, ready }] }` | all |
| `game_started` | `{ ends_at }` | all |
| `view_update` | `{ puzzle_index, view }` (that player's projection) | each |
| `action_result` | `{ outcome: "progress" \| "wrong" \| "solved", message? }` | each |
| `signal_received` | `{ from, kind, value }` | partner(s) |
| `hint` | `{ puzzle_index, level, text }` | all players |
| `puzzle_solved` | `{ puzzle_index, next_index }` | all |
| `room_completed` | `{ escaped: bool, time_secs, stats }` | all |
| `player_connection` | `{ nickname, connected }` | all |
| `host_update` | `HostView` | host |
| `error` | `{ code, message }` | sender |

### 7.3 Reconnection
- When a player joins, the server returns a signed **player token** (`session_id`, `player_id`, expiry), which the client stores in `sessionStorage`.
- When a socket drops, the server marks the player `connected: false`. The game keeps running and the partner sees "Waiting for X…".
- The client reconnects with exponential backoff (0.5 s → 8 s) using the same token, and the server replies with a fresh `welcome` snapshot.
- If both players have been disconnected for more than 2 minutes, the timer pauses.

---

## 8. REST API

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/auth/register` | — | Create a host account |
| POST | `/api/auth/login` | — | Log in (sets an HTTP-only cookie) |
| POST | `/api/auth/logout` | host | |
| GET | `/api/me` | host | Current host |
| GET | `/api/rooms` | host | List available room templates |
| POST | `/api/sessions` | host | Create a session from a room template; returns the room code + host WS token |
| GET | `/api/sessions` | host | Past sessions |
| GET | `/api/sessions/:id/report` | host | End-of-session report |
| DELETE | `/api/sessions/:id` | host | Delete a session and its events |
| POST | `/api/join` | — | `{ code, nickname }` → `{ player_token }` (rate-limited) |
| GET | `/api/health` | — | Health check |

---

## 9. Database schema (PostgreSQL)

```sql
CREATE TABLE hosts (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email         TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  display_name  TEXT NOT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE room_templates (
  id          TEXT PRIMARY KEY,           -- e.g. 'carthage-01'
  title       TEXT NOT NULL,
  definition  JSONB NOT NULL,             -- parsed YAML from content/rooms
  created_by  UUID REFERENCES hosts(id),  -- NULL = built-in
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TYPE session_status AS ENUM ('lobby', 'running', 'paused', 'escaped', 'failed', 'abandoned');

CREATE TABLE game_sessions (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code         TEXT NOT NULL,             -- 6 chars, unique among active sessions
  host_id      UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
  template_id  TEXT NOT NULL REFERENCES room_templates(id),
  seed         BIGINT NOT NULL,
  status       session_status NOT NULL DEFAULT 'lobby',
  started_at   TIMESTAMPTZ,
  ended_at     TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX active_code_idx ON game_sessions(code)
  WHERE status IN ('lobby', 'running', 'paused');

CREATE TABLE session_players (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id  UUID NOT NULL REFERENCES game_sessions(id) ON DELETE CASCADE,
  nickname    TEXT NOT NULL,
  role_index  SMALLINT NOT NULL,
  joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (session_id, role_index)
);

CREATE TABLE session_events (
  session_id  UUID NOT NULL REFERENCES game_sessions(id) ON DELETE CASCADE,
  seq         BIGINT NOT NULL,
  at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  player_id   UUID REFERENCES session_players(id),
  kind        TEXT NOT NULL,              -- 'player_joined', 'action', 'wrong', 'solved', 'hint', ...
  payload     JSONB NOT NULL DEFAULT '{}',
  PRIMARY KEY (session_id, seq)
);
```

Notes:
- Live state lives **in memory** in the room actor. The DB holds the durable record (session row + events).
- The events are written in batches (flushed every 1 s or 50 events) to avoid one insert per action.
- On a clean shutdown, active rooms flush their events. Recovering rooms after a crash is a stretch goal (rebuild the state by replaying events with the same seed).

---

## 10. Host dashboard and report

**Live dashboard (WebSocket `host_update`):**
- Room code shown in large type, with the lobby list and a Start button.
- Current puzzle, time elapsed on it, total time left.
- Per player: connected status, number of actions, wrong attempts, signals sent.
- Hints used.

**End-of-session report (computed from `session_events`):**
- Escaped or not, with the total time.
- Per puzzle: time taken, wrong attempts, hints used, and which player submitted the solution.
- Communication balance: the share of signals and actions per player. This flags sessions where one kid did everything.
- Topics practised (from the puzzle types), so the teacher can see which skills were hard.

---

## 11. Repository structure

```
carthage-escape/
├── PROJECT_SPEC.md            # this file
├── docker-compose.yml         # postgres + server (+ caddy in prod)
├── .github/workflows/ci.yml
├── content/
│   ├── rooms/carthage-01.yaml
│   └── decks/carthage_history.yaml
├── server/                    # Rust (cargo workspace)
│   ├── Cargo.toml
│   ├── migrations/            # sqlx migrations
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── error.rs
│       ├── api/               # REST handlers: auth, sessions, join, reports
│       ├── ws/                # WS upgrade, connection task, message (de)serialization
│       ├── game/
│       │   ├── manager.rs     # RoomManager
│       │   ├── room.rs        # Room actor + pure room logic
│       │   ├── protocol.rs    # ClientMsg / ServerMsg (ts-rs exported)
│       │   ├── signals.rs     # communication wheel + rate limiting
│       │   └── puzzles/
│       │       ├── mod.rs     # PuzzleType trait + AnyPuzzle enum
│       │       ├── lock_and_manual.rs
│       │       ├── blind_maze.rs
│       │       ├── translator.rs
│       │       ├── balance_scale.rs
│       │       └── timeline.rs
│       ├── content.rs         # YAML room loading + validation
│       ├── db/                # sqlx queries, event writer
│       └── auth.rs
└── web/                       # React + TS + Vite
    ├── package.json
    └── src/
        ├── generated/         # ts-rs output (do not edit)
        ├── net/socket.ts      # WS client: reconnect, heartbeat, seq tracking
        ├── store/             # zustand stores (game, host)
        ├── i18n/              # en.json (fr.json, ar.json later)
        ├── pages/             # Home, Join, Lobby, Game, HostLogin, HostDashboard, Report
        ├── components/        # CommWheel, Timer, HintButton, ...
        └── puzzles/           # one view component per puzzle type
            ├── LockAndManual/
            ├── BlindMaze/     # PixiJS
            ├── Translator/
            ├── BalanceScale/
            └── Timeline/
```

---

## 12. Roadmap and milestones

Each milestone ends with something **playable or demoable**, plus passing tests and a commit.

### M0: Project skeleton (≈ 2 days)
- Cargo project, Vite app, Docker Compose with Postgres, sqlx migrations running.
- `/api/health` works, and the frontend calls it.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run lint`, `npm run build`.
- **Done when:** `docker compose up` starts everything and CI is green.

### M1: Host auth + lobby (≈ 4 days)
- Host register and login (Argon2, cookie session).
- Host creates a session and gets a room code.
- Players join with the code + a nickname, receive a token and open a WebSocket.
- Room actor + RoomManager; the lobby shows players live; the host clicks Start.
- `ts-rs` type generation wired into the frontend build.
- **Done when:** 2 browser windows join the same room and see each other in the lobby; the host starts the game.

### M2: First puzzle end to end (≈ 4 days)
- `PuzzleType` trait + `lock_and_manual`.
- Separate player views; the server validates actions; wrong/solved feedback.
- Unit tests including **"no secret in any view"**.
- **Done when:** two players solve the Lock & Manual puzzle by talking to each other.

### M3: Puzzle engine + full room (≈ 2 weeks)
- YAML room loading and validation; seeded RNG.
- The remaining puzzle types: `translator`, `balance_scale`, `timeline`, `blind_maze` (PixiJS).
- Puzzle chain progression, timer, hints with time penalty, escaped/failed end screens.
- **Done when:** the full Carthage room is playable from start to finish.

### M4: Communication wheel + reconnection (≈ 1 week)
- Radial wheel UI, signals, pings, rate limiting.
- Player tokens, reconnect with backoff, a `welcome` snapshot, partner "waiting" state, pause when everyone is gone.
- **Done when:** you can kill a player's tab mid-puzzle, reopen it, and continue seamlessly.

### M5: Host dashboard + reports (≈ 1 week)
- Batched event writer to `session_events`.
- Live `host_update`; report endpoint and page.
- **Done when:** after a game, the host sees per-puzzle stats and the communication balance.

### M6: Polish + ship (≈ 1–2 weeks)
- Art direction (Carthage theme: mosaics, ships, Punic symbols), sounds, animations, a kid-friendly UI (big tap targets, readable fonts).
- Playwright E2E test: two browser contexts play through a room.
- Production Docker image (multi-stage), Caddy with HTTPS, deployed on a VPS, CI deploy on tag.
- README with screenshots, an architecture diagram and a **1-minute demo video**.
- **Done when:** a live public URL works on a tablet.

---

## 13. Testing strategy
- **Puzzle unit tests (most important):** determinism with a seed, no secret leakage in views, correct and wrong actions, edge cases.
- **Room logic tests:** the room logic is a pure function, so test command sequences such as join → start → actions → solved → completed, and disconnect/reconnect.
- **API integration tests:** using `sqlx::test` with a throwaway DB.
- **Protocol tests:** serialization round-trips of every `ClientMsg` and `ServerMsg`.
- **Frontend:** Vitest for stores and the socket client (reconnection logic).
- **E2E:** Playwright with 2 contexts as 2 players plus 1 as the host.
- **Load test (stretch):** a small Rust or k6 script that simulates 200 rooms × 2 bots to measure server CPU and memory.

---

## 14. Conventions
- **Server is authoritative.** The client never decides whether an answer is correct.
- **Never send answers or other players' secrets to a client.** Every new puzzle type needs a leak test.
- Room logic must stay **pure and synchronous**. I/O (sockets, DB) happens only in the actor shell.
- Protocol types are defined in Rust and exported to TypeScript. Never hand-write duplicate types on the frontend.
- No `unwrap()` in server code outside tests. Use `thiserror` for errors and `anyhow` only in `main`.
- Commits are small, using conventional-commit messages (`feat(game): add translator puzzle`).
- Every milestone: tests pass, `cargo clippy` is clean, and the README is updated.
- All UI strings go through i18n. No hardcoded text in components.

---

## 15. Open questions / decisions to make later
- Exact age range and reading level (this affects the manual's wording and puzzle difficulty).
- Whether to support 3–4 players in v1, or keep strictly 2.
- Art: draw it myself, use free asset packs (check the licenses), or go fully stylized/minimal.
- Whether the puzzle editor stores content in the DB only, or also exports YAML.
- Hosting: a cheap VPS (Hetzner or Oracle free tier) vs a PaaS.

---

## 16. What this project demonstrates (for the portfolio / PFE interviews)
- Designing an **authoritative real-time multiplayer server** in Rust (actor model, WebSockets).
- **Security by design:** per-player state projection, server-side validation, kid-safe communication.
- **A data-driven engine:** puzzles as types plus content as data, with seeded deterministic generation.
- **Resilience:** reconnection, snapshots, sequence numbers.
- **Event sourcing** feeding real analytics for teachers.
- **Full delivery:** testing at every level, CI/CD, Docker, a production deployment.
- **Product thinking:** a real educational use case, accessibility, i18n including RTL.
