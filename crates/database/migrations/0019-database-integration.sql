-- Gravity v3.4.0 production database integration
-- Durable generic persistence table used by the hot/cold write queue.
CREATE TABLE IF NOT EXISTS persistence_records (
  id BIGSERIAL PRIMARY KEY,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_persistence_records_kind_target ON persistence_records(kind, target);
CREATE INDEX IF NOT EXISTS idx_persistence_records_sequence ON persistence_records(sequence);

CREATE TABLE IF NOT EXISTS storage_health_checks (
  id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  postgres_ok BOOLEAN NOT NULL,
  redis_ok BOOLEAN NOT NULL,
  queued BIGINT NOT NULL,
  capacity BIGINT NOT NULL,
  dropped BIGINT NOT NULL,
  checked_ms BIGINT NOT NULL,
  body JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS schema_migrations (
  file TEXT PRIMARY KEY,
  applied_ms BIGINT NOT NULL,
  checksum TEXT NOT NULL DEFAULT ''
);
