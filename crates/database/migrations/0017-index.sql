-- Gravity v2.7 index fund runtime schema shell
CREATE TABLE IF NOT EXISTS index_products (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    quote_asset TEXT NOT NULL,
    management_fee_bps INTEGER NOT NULL,
    rebalance_threshold_bps INTEGER NOT NULL,
    created_ms BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS index_nav_reports (
    id BIGSERIAL PRIMARY KEY,
    product_id TEXT NOT NULL,
    nav TEXT NOT NULL,
    nav_per_unit TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    created_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS index_rebalance_plans (
    id BIGSERIAL PRIMARY KEY,
    product_id TEXT NOT NULL,
    required BOOLEAN NOT NULL,
    sequence BIGINT NOT NULL,
    body JSONB NOT NULL,
    created_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS index_events (
    id BIGSERIAL PRIMARY KEY,
    product_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    body JSONB NOT NULL,
    created_ms BIGINT NOT NULL
);
