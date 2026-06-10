-- Gravity v2.2 risk runtime tables.
CREATE TABLE IF NOT EXISTS risk_snapshots (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  status TEXT NOT NULL,
  health_factor_bps NUMERIC NOT NULL,
  collateral_value_raw TEXT NOT NULL,
  discounted_collateral_value_raw TEXT NOT NULL,
  position_notional_raw TEXT NOT NULL,
  debt_value_raw TEXT NOT NULL,
  equity_raw TEXT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_risk_snapshots_account ON risk_snapshots(account);
CREATE INDEX IF NOT EXISTS idx_risk_snapshots_status ON risk_snapshots(status);

CREATE TABLE IF NOT EXISTS risk_events (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  health_factor_bps NUMERIC NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  message TEXT NOT NULL,
  body JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_risk_events_account ON risk_events(account);
CREATE INDEX IF NOT EXISTS idx_risk_events_timestamp ON risk_events(timestamp_ms);
