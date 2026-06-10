# Security Policy

Gravity is financial infrastructure. Treat correctness, replay safety, and deterministic accounting as security boundaries.

## Reporting issues

Please do not disclose security issues publicly until they are triaged. For now, report privately to the project maintainer.

## Critical areas

- Fixed-point overflow/rounding
- CLOB price-time priority
- Settlement idempotency and asset conservation
- Oracle stale/outlier handling
- Risk/liquidation safety
- WAL replay determinism
- JIT/native equivalence
- API idempotency and request validation

## Deployment notes

- Review all files in `config/` before production use.
- Do not store secrets in checked-in config files.
- Run the release gate before packaging.
- Treat JIT acceleration as optional until equivalence tests pass on the deployment target.
