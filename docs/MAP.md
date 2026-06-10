
## v2.1.0 Native AMM Runtime

Added Gravity AMM runtime with pool creation, quotes, swaps, liquidity hooks, AMM events, settlement hints, and `amm-quote` benchmarks.

# Gravity Project Map

```text
gravity/
  config/
  crates/
    types/        fixed-point financial primitives and shared events
    config/       config loading and validation
    market/       market event bus and adapter traits
    adapters/     optional exchange adapter normalizers
    book/         native CLOB/orderbook engine
    oracle/       oracle aggregation/signing scaffold
    settlement/   Stargate settlement placeholder
    database/     memory/Postgres/Redis storage boundaries
    api/          Axum API and WebSocket routes
    core/         processor/oracle/settlement orchestration
    worker/       feed, processor, and market worker orchestration
  apps/
    gravityd/     main service binary
    gravitybench/ CLOB benchmark binary
  runtime/
  scripts/
  docs/
```

- `docs/RUNTIME.md` — runtime stream, persistence, and benchmark report behavior.


## v1.0 Docs

- `AUDIT.md` - deterministic service audit trail.
- `REPLAY.md` - replay/audit rebuilding plan.
- `SETTLEMENT-BATCHING.md` - settlement batch shell.


## v1.0 Market Worker Runtime

Gravity v1.0 adds per-market single-writer CLOB workers, bounded queues, cached depth snapshots, fixed-size event rings, worker metrics, and batch order intake.

## v1.3 Runtime Additions

- `crates/settlement` includes compressed settlement batches and net deltas.
- `crates/database` includes bounded hot/cold persistence records.
- `crates/api` exposes persistence operational endpoints.
- `apps/gravitybench` includes a settlement compression benchmark phase.


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


## v1.9.0 Validation Gate

See `docs/VALIDATION-GATE.md` for the release validation flow, benchmark target verdicts, and report checks before the v1.8 Production CLOB pass.


## v2.0.0 Production Oracle Runtime

Added production oracle source health, confidence scoring, expanded aggregation modes, signed report hardening, oracle stats/source APIs, migration shell, and benchmark coverage.


## v2.2.0 Risk Runtime

Added `crates/risk`, risk APIs, risk benchmark coverage, and risk persistence migration shells.


## Gravity v2.3.0 Liquidation Runtime

Added liquidation candidate scanning, partial/full liquidation planning, priority scoring, API routes, benchmark coverage, config, migration shell, and docs.


## v2.4.0 Durable Replay/WAL Runtime

Added `gravity-wal`, append-only WAL streams, checkpoint records, replay-plan APIs, and WAL metadata migration shells.


## v2.5.0 AMM Hardening

AMM now includes stable/weighted pool policy, LP removal, oracle guards, and impact guards.


## v2.6.0 - Perps Engine Foundation

- Added `gravity-perps` crate.
- Added perpetual futures markets, position opening, funding updates, mark/index pricing, PnL/equity/margin calculations, liquidation price hints, API routes, benchmark coverage, config, migration shell, and docs.



## v2.7.0 Index Fund Engine

Added index product runtime, NAV/rebalance/mint/redeem planners, API routes, migration shell, and benchmark coverage.

## Tile Runtime Completion

- `crates/tile/` owns tile supervisor, health, pinning, and tuning primitives.
- `GET /tiles` and `GET /tiles/health` expose operational status.

## v3.0.0 additions

- `config/jit.toml` — safe JIT kernel guardrails.
- `docs/JIT-KERNELS.md` — deterministic JIT kernel design.
- `crates/tile` — JIT registry, kernel descriptors, equivalence checks.


## v3.1.0 Hardware-Aware Runtime

Adds machine profiling, placement plans, hardware profiles, pinning hints, and hardware-placement benchmark coverage.

- `docs/SECURITY-CORRECTNESS.md` — v3.3 correctness, invariant, and fail-closed testing plan.
- `config/security.toml` — correctness/security validation policy scaffold.


## v3.6.0 Ops Layer

Adds `/ops` read-only runtime visibility for startup, health, queues, latency, services, and performance.


## Performance layer

- `crates/perf` — interners, object pools, reusable vectors, fast counter maps, and optimization benchmark primitives.
