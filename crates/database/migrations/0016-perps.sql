CREATE TABLE IF NOT EXISTS perp_markets (
  symbol TEXT PRIMARY KEY,
  index_symbol TEXT NOT NULL,
  body JSONB NOT NULL,
  updated_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS perp_positions (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  symbol TEXT NOT NULL,
  side TEXT NOT NULL,
  quantity_raw TEXT NOT NULL,
  entry_price_raw TEXT NOT NULL,
  mark_price_raw TEXT NOT NULL,
  collateral_raw TEXT NOT NULL,
  equity_raw TEXT NOT NULL,
  body JSONB NOT NULL,
  updated_ms BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_perp_positions_account ON perp_positions(account);
CREATE INDEX IF NOT EXISTS idx_perp_positions_symbol ON perp_positions(symbol);

CREATE TABLE IF NOT EXISTS perp_events (
  id BIGSERIAL PRIMARY KEY,
  kind TEXT NOT NULL,
  account TEXT,
  symbol TEXT NOT NULL,
  position_id TEXT,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  message TEXT NOT NULL,
  body JSONB NOT NULL
);
