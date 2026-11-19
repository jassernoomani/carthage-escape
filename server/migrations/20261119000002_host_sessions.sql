-- Login sessions for hosts. The cookie holds a random token; only its SHA-256 hash is stored,
-- so a database leak does not leak usable session cookies.
CREATE TABLE host_sessions (
  token_hash  BYTEA PRIMARY KEY,
  host_id     UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX host_sessions_host_idx ON host_sessions(host_id);
