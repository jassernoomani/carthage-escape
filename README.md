# Carthage Escape

A real-time, co-op educational escape room for kids aged 8–12. Each player sees different clues, so no one can solve a puzzle alone. See [PROJECT_SPEC.md](PROJECT_SPEC.md) for the full design.

**Status:** M0 (project skeleton) complete.

## Stack
- **Server:** Rust (axum, tokio, sqlx) in [`server/`](server/)
- **Web:** React + TypeScript + Vite + Tailwind in [`web/`](web/)
- **Database:** PostgreSQL 16
- **Shared types:** Rust structs exported to TypeScript with `ts-rs` (into `web/src/generated/`)

## Quick start (everything in Docker)
```sh
docker compose up --build
```
Then open http://localhost:8080.

## Development
Prerequisites: Rust (stable, MSVC toolchain on Windows), Node 24+, Docker.

```sh
cp .env.example .env
docker compose up -d db          # Postgres on :5432

cd server
cargo run                        # API on :8080, runs migrations on startup

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

### Database migrations
Migrations live in `server/migrations/` and run automatically when the server starts. To add one:
```sh
cd server && sqlx migrate add <name>
```
