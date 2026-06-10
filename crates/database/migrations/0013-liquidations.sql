-- Gravity v2.3.0 liquidation runtime shell
CREATE TABLE IF NOT EXISTS liquidation_candidates (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  status TEXT NOT NULL,
  health_factor_bps BIGINT NOT NULL,
  priority_score BIGINT NOT NULL,
  body JSONB NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS liquidation_events (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  kind TEXT NOT NULL,
  body JSONB NOT NULL,
  created_at BIGINT NOT NULL
);
