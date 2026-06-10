-- Gravity v1.9.0 settlement finalization records.
CREATE TABLE IF NOT EXISTS settlement_batches (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    symbol TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    payload_root TEXT NOT NULL,
    audit_root TEXT,
    status TEXT NOT NULL,
    reference TEXT,
    attempts BIGINT NOT NULL DEFAULT 0,
    created_ms BIGINT NOT NULL,
    updated_ms BIGINT NOT NULL,
    body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_settlement_batches_symbol_updated ON settlement_batches(symbol, updated_ms DESC);
CREATE INDEX IF NOT EXISTS idx_settlement_batches_status_updated ON settlement_batches(status, updated_ms DESC);

CREATE TABLE IF NOT EXISTS settlement_dead_letters (
    id BIGSERIAL PRIMARY KEY,
    batch_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    reason TEXT NOT NULL,
    attempts BIGINT NOT NULL DEFAULT 0,
    created_ms BIGINT NOT NULL,
    updated_ms BIGINT NOT NULL,
    body JSONB NOT NULL
);
