
## v2.1.0 Native AMM Runtime

Added Gravity AMM runtime with pool creation, quotes, swaps, liquidity hooks, AMM events, settlement hints, and `amm-quote` benchmarks.

# Gravity Roadmap

## Current: v0.8.0 Runtime Hardening

Gravity now has a stronger runtime shell: probes, WebSocket snapshots, dependency upgrades, graceful shutdown, per-market worker scaffolding, and a real CLOB benchmark binary.

## Upcoming v0.8.0 Recommended Work

1. Live WebSocket exchange clients behind the existing adapter boundary.
2. Real broadcast channels for book/trade/oracle streaming instead of snapshot polling.
3. Persistent SQLx writes for every order lifecycle event.
4. Redis hot-depth cache reads before memory fallback.
5. Market-worker configuration from `markets.toml` instead of the default BTC/ETH pair list.
6. Prometheus-compatible metrics endpoint.
7. CLOB stress tests for insert/match/cancel/snapshot under sustained load.
8. Signed settlement batch builder for Stargate L3 markets.


## v1.0 complete

Runtime execution/audit hardening, batch settlement shell, stricter order validation, and replay documentation.

## v1.0 target

Durable Postgres write path by default, Redis fanout under load, real WebSocket exchange feeds, and risk engine foundation.


## v1.0 Market Worker Runtime

Gravity v1.0 adds per-market single-writer CLOB workers, bounded queues, cached depth snapshots, fixed-size event rings, worker metrics, and batch order intake.


## v1.2 Completed

- Hot-path grouped batch order intake.
- Multi-depth cached snapshots.
- Batch benchmark phase and stronger summary reports.

## Upcoming Candidate: v1.3

- Hot/cold async persistence writer queue.
- Settlement batch compression with net balance deltas.
- Binary internal worker frames.
- Per-market load shedding policies.

## After v1.3

Recommended performance upgrades:

1. background cold-writer tasks that drain persistence records to Postgres in batches;
2. binary internal message frames for worker commands and market events;
3. per-market settlement compressor windows with configurable max fills/max age;
4. lock-free or sharded rings for recent book events;
5. hardware-aware worker placement for high-volume markets.


## v1.4.0 Wire Layer

Added `crates/wire` for compact binary hot-path frames and benchmark coverage.


## v1.5.0 Binary Intake + Parallel Execution

- Binary order intake endpoints.
- Order batch wire frames.
- Parallel grouped batch dispatch across market workers.
- Wire-batch and parallel-market benchmark phases.
- Profiling helper scripts.


## v1.6 Tile Runtime Additions

Gravity now includes a dedicated tile runtime layer: crossbeam queues, tile roles, CPU pinning, adaptive batch tuning, and a Cranelift JIT feature boundary for future deterministic hot kernels.


## After v1.7

Recommended follow-up: preallocated order/fill pools, zero-copy stream fanout, and hardware-aware market scheduler tuning.


## v1.9.0 Validation Gate

See `docs/VALIDATION-GATE.md` for the release validation flow, benchmark target verdicts, and report checks before the v1.8 Production CLOB pass.

## v1.9.0 Production CLOB Engine

Completed the production CLOB behavior pass: amend, cancel-replace, market status controls, STP, fees, and market rules.

## v1.9.0 Completed Focus

CLOB correctness coverage was expanded. The recommended following release is `v1.9.0 Production Settlement Engine`.

## v1.9.0 Completed Direction

Settlement now has a production finalization boundary: compressed fill batches, idempotency keys, payload roots, recent records, dead-letter retry, and Stargate instruction scaffolding. The next major production layer is v2.0.0 Production Oracle Runtime.


## v2.2.0 Risk Runtime

Added `crates/risk`, risk APIs, risk benchmark coverage, and risk persistence migration shells.


## Gravity v2.3.0 Liquidation Runtime

Added liquidation candidate scanning, partial/full liquidation planning, priority scoring, API routes, benchmark coverage, config, migration shell, and docs.


## v2.4.0 Durable Replay/WAL Runtime

Added `gravity-wal`, append-only WAL streams, checkpoint records, replay-plan APIs, and WAL metadata migration shells.


## v2.5.0 Complete

AMM production hardening added. Upcoming: perps/index/synthetics, production streaming, tile runtime completion, safe JIT kernels.


## v2.6.0 - Perps Engine Foundation

- Added `gravity-perps` crate.
- Added perpetual futures markets, position opening, funding updates, mark/index pricing, PnL/equity/margin calculations, liquidation price hints, API routes, benchmark coverage, config, migration shell, and docs.



## v2.7.0 Index Fund Engine

Added index product runtime, NAV/rebalance/mint/redeem planners, API routes, migration shell, and benchmark coverage.

## v3.0.0 Complete

Tile Runtime Completion is in place. Upcoming work: v3.0 Safe Cranelift JIT kernels.


## v3.1.0 Hardware-Aware Runtime

Adds machine profiling, placement plans, hardware profiles, pinning hints, and hardware-placement benchmark coverage.


## v3.2.0 Completed

- WAL checkpoint loading and recovery reporting.
- Dry-run replay validation.
- Corruption/degraded verdicts for production startup tooling.

## Upcoming

- v3.3.0 Security and financial correctness hardening.
- v3.4.0 Production database integration.
- v3.5.0 API/SDK contract finalization.


## Completed v3.6.0

Observability + Ops Console with read-only operational endpoints and configuration.


## Completed v3.7.0

- Final performance optimization foundation.
- Interning, reusable object pools, reusable vectors, and fast aggregation maps.
- Performance-final config and benchmark visibility.
