CREATE TABLE IF NOT EXISTS audit_records (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  payload_hash TEXT NOT NULL,
  message TEXT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_records_target_time ON audit_records(target, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_audit_records_kind_time ON audit_records(kind, timestamp_ms DESC);
