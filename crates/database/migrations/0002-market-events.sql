CREATE TABLE IF NOT EXISTS market_events (
  id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  venue TEXT NOT NULL,
  kind TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL,
  inserted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(symbol, venue, sequence, kind)
);

CREATE INDEX IF NOT EXISTS market_events_symbol_time_idx ON market_events(symbol, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS market_events_venue_seq_idx ON market_events(venue, sequence DESC);
