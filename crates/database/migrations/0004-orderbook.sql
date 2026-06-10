CREATE TABLE IF NOT EXISTS orders (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  symbol TEXT NOT NULL,
  side TEXT NOT NULL,
  kind TEXT NOT NULL,
  tif TEXT NOT NULL,
  price_raw TEXT,
  quantity_raw TEXT NOT NULL,
  remaining_raw TEXT NOT NULL,
  status TEXT NOT NULL,
  client_id TEXT,
  created_ms BIGINT NOT NULL,
  updated_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS orders_symbol_status_idx ON orders(symbol, status);
CREATE INDEX IF NOT EXISTS orders_account_idx ON orders(account);

CREATE TABLE IF NOT EXISTS fills (
  id TEXT PRIMARY KEY,
  symbol TEXT NOT NULL,
  maker_order TEXT NOT NULL,
  taker_order TEXT NOT NULL,
  maker_account TEXT NOT NULL,
  taker_account TEXT NOT NULL,
  price_raw TEXT NOT NULL,
  quantity_raw TEXT NOT NULL,
  taker_side TEXT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS fills_symbol_time_idx ON fills(symbol, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS fills_taker_idx ON fills(taker_account);
CREATE INDEX IF NOT EXISTS fills_maker_idx ON fills(maker_account);
