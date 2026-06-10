-- Gravity v2.0.0 production oracle runtime
CREATE TABLE IF NOT EXISTS oracle_sources (
  symbol TEXT NOT NULL,
  venue TEXT NOT NULL,
  price_raw TEXT NOT NULL,
  quantity_raw TEXT,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  age_ms BIGINT NOT NULL,
  deviation_bps INTEGER,
  status TEXT NOT NULL,
  kind TEXT NOT NULL,
  body JSONB NOT NULL,
  PRIMARY KEY(symbol, venue)
);

CREATE TABLE IF NOT EXISTS oracle_audit (
  id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  report_timestamp_ms BIGINT NOT NULL,
  method TEXT NOT NULL,
  sources INTEGER NOT NULL,
  confidence_bps INTEGER NOT NULL,
  payload_hash TEXT NOT NULL,
  body JSONB NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);
