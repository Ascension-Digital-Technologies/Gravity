-- Gravity v1.8 production CLOB extension shell.
-- Fills already store full JSON body; fee columns are optional materialized fields.
ALTER TABLE fills ADD COLUMN IF NOT EXISTS maker_fee_quote_raw TEXT;
ALTER TABLE fills ADD COLUMN IF NOT EXISTS taker_fee_quote_raw TEXT;

CREATE TABLE IF NOT EXISTS order_amendments (
  id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  order_id TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS market_status_events (
  id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  status TEXT NOT NULL,
  timestamp_ms BIGINT NOT NULL
);
