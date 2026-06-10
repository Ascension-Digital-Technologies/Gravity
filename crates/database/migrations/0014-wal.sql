-- Gravity v2.4 WAL/replay metadata shell.
CREATE TABLE IF NOT EXISTS wal_checkpoints (
  id TEXT PRIMARY KEY,
  created_at_ms BIGINT NOT NULL,
  last_sequence BIGINT NOT NULL,
  note TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS wal_replay_runs (
  id BIGSERIAL PRIMARY KEY,
  started_at_ms BIGINT NOT NULL,
  finished_at_ms BIGINT,
  mode TEXT NOT NULL,
  status TEXT NOT NULL,
  last_sequence BIGINT NOT NULL DEFAULT 0,
  details JSONB NOT NULL DEFAULT '{}'::jsonb
);
