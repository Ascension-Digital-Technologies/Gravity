CREATE TABLE IF NOT EXISTS persistence_records (
  id BIGSERIAL PRIMARY KEY,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_persistence_records_kind_ts ON persistence_records(kind, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_persistence_records_target_ts ON persistence_records(target, timestamp_ms DESC);
