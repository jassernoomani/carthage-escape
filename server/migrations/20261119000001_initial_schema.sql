-- Initial schema (PROJECT_SPEC.md §9).

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
CREATE INDEX game_sessions_host_idx ON game_sessions(host_id, created_at DESC);

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
  player_id   UUID REFERENCES session_players(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL,              -- 'player_joined', 'action', 'wrong', 'solved', 'hint', ...
  payload     JSONB NOT NULL DEFAULT '{}',
  PRIMARY KEY (session_id, seq)
);
