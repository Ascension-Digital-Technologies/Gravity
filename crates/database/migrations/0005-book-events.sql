CREATE TABLE IF NOT EXISTS book_events (
  id BIGSERIAL PRIMARY KEY,
  kind TEXT NOT NULL,
  symbol TEXT NOT NULL,
  order_id TEXT NOT NULL,
  fill_id TEXT,
  price_raw TEXT,
  quantity_raw TEXT,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  message TEXT NOT NULL,
  body JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS book_events_symbol_seq_idx ON book_events(symbol, sequence DESC);
CREATE INDEX IF NOT EXISTS book_events_symbol_time_idx ON book_events(symbol, timestamp_ms DESC);
