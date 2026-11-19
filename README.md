# Carthage Escape

A real-time, co-op educational escape room for kids aged 8–12. Each player sees different clues, so no one can solve a puzzle alone. See [PROJECT_SPEC.md](PROJECT_SPEC.md) for the full design.

**Status:** M2 (puzzle engine + first puzzle) complete. A host creates a game, two kids join with the room code, and together they open the **Lock & Manual** puzzle: one sees colored number tiles, the other the manual that explains how to turn them into the code, so they can only solve it by talking. The other four Carthage puzzles arrive in M3.

## Stack
- **Server:** Rust (axum, tokio, sqlx) in [`server/`](server/)
- **Web:** React + TypeScript + Vite + Tailwind in [`web/`](web/)
- **Database:** PostgreSQL 16
- **Shared types:** Rust structs exported to TypeScript with `ts-rs` (into `web/src/generated/`)

## How it fits together
- **Hosts** (teachers/parents) log in with email + password (Argon2). The session cookie is HTTP-only; only a SHA-256 hash of its token is stored.
- **Players** never have accounts: `POST /api/join` with a room code + nickname returns a signed token, kept in `sessionStorage`. Nicknames are filtered (16 chars max, blocklist, no phone numbers or emails). A kid who closed the tab gets their seat back by typing the same nickname (only while that seat is disconnected). The host can remove players from the lobby.
- **Each live game is an actor**: a tokio task owning the room state. The logic is a pure function (`server/src/game/room.rs`: `(state, input) -> effects`); the actor shell (`actor.rs`) does all the I/O.
- **Puzzle engine** (`server/src/game/puzzles/`): each puzzle type implements `PuzzleType` (`init` from params + seed, `view_for(role)`, `apply`, `hint`). Rooms list puzzles as data in `content/rooms/*.yaml`. Generation is deterministic per session seed, roles rotate along the chain, and every type has a test that **no player's view ever contains the answer**.
- **WebSocket** `GET /ws?token=…` carries the protocol in `server/src/game/protocol.rs`. Every server message has a `seq`. Clients send heartbeats and the server pings (so throttled background tabs stay connected); clients reconnect with backoff (0.5 s → 8 s) and get a full `welcome` snapshot on every connect. Fatal errors (bad token, removed, room closed) end with close code 4000 so clients stop retrying.
- **Hardening:** per-IP rate limits sized for a whole school behind one IP, per-account login limit, JSON errors everywhere, CSP and other security headers, compressed and long-cached static assets, tokens kept out of logs.

## Quick start (everything in Docker)
```sh
docker compose up --build
```
Then open http://localhost:8080.

## Development
Prerequisites: Rust (stable, MSVC toolchain on Windows), Node 24+, Docker, `sqlx-cli`
(`cargo install sqlx-cli --no-default-features --features postgres,rustls`).

```sh
cp .env.example .env
docker compose up -d db          # Postgres on :5432

cd server
cargo run                        # API on :8080, runs migrations and loads content/rooms on startup

cd web
npm install
npm run dev                      # http://localhost:5173, proxies /api and /ws to :8080
```

### Checks (same as CI)
```sh
# server/ (needs the db container running)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # also regenerates web/src/generated/*.ts

# web/
npm run lint
npm test
npm run build
```

### Shared types
Rust protocol/API types are exported to `web/src/generated/` by ts-rs. Regenerate after changing them (CI fails if they're stale):
```sh
cd web && npm run gen:types
```

### End-to-end test
Runs a host and two players in separate browser contexts against a running stack (also runs in CI):
```sh
docker compose up --build -d
cd web && PW_CHANNEL=msedge npm run e2e    # or PW_CHANNEL=chrome
```

### Production settings
| Variable | Purpose |
|---|---|
| `JWT_SECRET` | 32+ random bytes for signing player/host tokens (required) |
| `COOKIE_SECURE=true` | when served over HTTPS |
| `TRUST_PROXY=true` | only behind exactly one reverse proxy (Caddy) that sets `X-Forwarded-For` |

### Database queries and migrations
Queries are checked at compile time with `sqlx::query!`. After adding or changing a query, refresh the offline cache (used by Docker and CI) and commit `server/.sqlx/`:
```sh
cd server && cargo sqlx prepare -- --all-targets
```
Migrations live in `server/migrations/` and run automatically when the server starts. To add one:
```sh
cd server && sqlx migrate add <name>
```
