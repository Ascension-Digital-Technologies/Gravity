CREATE TABLE IF NOT EXISTS oracle_reports (
  symbol TEXT PRIMARY KEY,
  price_raw TEXT NOT NULL,
  confidence_bps BIGINT NOT NULL,
  sources BIGINT NOT NULL,
  method TEXT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  key_id TEXT,
  payload_hash TEXT NOT NULL,
  signature TEXT,
  body JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS oracle_reports_timestamp_idx ON oracle_reports(timestamp_ms DESC);
