-- Gravity v2.1 AMM runtime migration shell
CREATE TABLE IF NOT EXISTS amm_pools (
  symbol TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  base_reserve NUMERIC NOT NULL,
  quote_reserve NUMERIC NOT NULL,
  lp_supply NUMERIC NOT NULL,
  fee_bps INTEGER NOT NULL,
  sequence BIGINT NOT NULL,
  updated_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS amm_events (
  id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  kind TEXT NOT NULL,
  sequence BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);
