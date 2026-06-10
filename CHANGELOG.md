# Changelog

## v3.7.0 — Final Performance Optimization

- Added `gravity-perf` crate with interners, reusable vectors, object pools, fast counter maps, and arena-style allocation helpers.
- Added `config/performance-final.toml`.
- Added `perf-pool` benchmark phase.
- Added final performance documentation and release manifest.

## v3.6.0 — Observability + Ops Console

- Added read-only ops endpoints under `/ops`.
- Added startup, health, queue, latency, service, and performance summaries.
- Added `config/ops.toml` and observability docs.

## v3.5.0 — Production API + SDK Contract

- Added API metadata endpoints: `/api`, `/api/version`, `/api/errors`, `/api/routes`, and `/api/openapi`.
- Added response-envelope, error-code, request-id, idempotency-key, pagination, SDK, and binary protocol documentation.
- Added OpenAPI starter document and API contract config.

## v3.4.1 — Database/Oracle Compile Fix

- Added missing `GravityStore` oracle source/stat delegation methods.
- Cleaned hardware target cfg warnings and perps unused import warning.

## v3.4.0 — Production Database Integration

- Added database health reporting for Postgres/Redis modes.
- Added migration validation, storage backpressure reporting, Redis PING health check, and persistence record migration shell.
- Added `/database`, `/database/migrations`, and `/database/backpressure` APIs.

## v3.3.0 — Security + Financial Correctness

- Added financial invariant tests across fixed-point types, CLOB, settlement, AMM, risk, perps, index, WAL, and JIT equivalence.
- Added correctness guardrail config and security/correctness docs.

## v3.2.0 — WAL Replay + Recovery Completion

- Added recovery reports, checkpoint loading, malformed-record detection, sequence-regression detection, and dry-run replay planning.
- Added `/wal/checkpoints`, `/wal/recovery-report`, and `/wal/replay-run`.

## v3.1.0 — Hardware-Aware Runtime

- Added hardware/profile planning crate and runtime placement profiles.
- Added `/hardware`, `/hardware/plan`, and `/hardware/simulate` APIs.

## v3.0.0 — Safe JIT Kernels

- Added deterministic JIT kernel registry scaffold with native fallback and exact-equivalence checks.
- Added benchmark phase for JIT kernel equivalence.

## v2.9.0 — Tile Runtime Completion

- Added tile supervisor, health, queue pressure metrics, restart-all path, and tile API routes.

## v2.8.0 — Production Streaming Layer

- Added shared JSON/binary stream hub, topic stats, recent replay records, and stream APIs.

## v2.7.0 — Index Fund Engine

- Added index products, NAV calculation, rebalance/mint/redeem planning, oracle dependency maps, and benchmark coverage.

## v2.6.0 — Perps Engine Foundation

- Added perp markets, positions, funding updates, mark/index pricing, PnL, margin, and liquidation price hints.

## v2.5.0 — AMM Production Hardening

- Added stable/weighted pool policy fields, LP remove-liquidity, price-impact guards, and oracle deviation checks.

## v2.4.0 — Durable Replay/WAL Runtime

- Added append-only WAL streams, checkpoint records, replay-plan APIs, and WAL migration shell.

## v2.3.0 — Liquidation Runtime

- Added liquidation scanning, planning, priority scoring, config, APIs, benchmarks, and migration shell.

## v2.2.0 — Risk Engine Runtime

- Added account health, collateral, margin, exposure, risk events, APIs, benchmarks, and migration shell.

## v2.1.x — AMM Runtime + Compile Fixes

- Added native AMM runtime and follow-up fixes for oracle serde, settlement ownership, and AMM worker context integration.

## v2.0.0 — Production Oracle Runtime

- Added source health, expanded aggregation modes, stale/outlier filtering, confidence scoring, oracle source/stat APIs, and benchmark coverage.

## v1.x — Market Runtime Foundations

- Added CLOB, market workers, settlement compression, binary wire, parallel execution, tile/JIT scaffold, validation gates, and production CLOB behavior.
