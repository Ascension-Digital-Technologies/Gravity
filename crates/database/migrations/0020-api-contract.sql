-- Gravity v3.5.0 API contract metadata shell.
CREATE TABLE IF NOT EXISTS api_contract_versions (
  id BIGSERIAL PRIMARY KEY,
  api_version TEXT NOT NULL,
  contract_version TEXT NOT NULL,
  created_ms BIGINT NOT NULL,
  notes TEXT NOT NULL DEFAULT ''
);
