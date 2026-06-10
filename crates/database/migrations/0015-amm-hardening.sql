-- Gravity v2.5.0 AMM production hardening shell

CREATE TABLE IF NOT EXISTS amm_liquidity_events (
    id BIGSERIAL PRIMARY KEY,
    symbol TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    body JSONB NOT NULL,
    created_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS amm_oracle_guards (
    id BIGSERIAL PRIMARY KEY,
    symbol TEXT NOT NULL,
    pool_price TEXT NOT NULL,
    oracle_price TEXT NOT NULL,
    deviation_bps INTEGER NOT NULL,
    max_deviation_bps INTEGER NOT NULL,
    allowed BOOLEAN NOT NULL,
    created_ms BIGINT NOT NULL
);
