# Gravity

Gravity is Ascension's Rust DeFi service layer: a high-performance backend for markets, oracles, settlement, risk, liquidations, AMMs, perps, index funds, streaming, WAL recovery, tile execution, and hardware-aware runtime planning.

Gravity is intentionally separate from Stargate's deterministic chain hot path. Gravity computes, routes, streams, batches, indexes, and prepares settlement payloads. Stargate/L3 remains the final source of truth.

## Status

**Version:** `3.7.0`  
**Current milestone:** Final performance optimization foundation  
**Build target:** Rust `1.77+`  
**License:** MIT

## Highlights

- Native CLOB/orderbook with price-time priority, market statuses, amend/cancel-replace, tick/lot/min validation, self-trade prevention, and maker/taker fee fields.
- Settlement finalization boundary with compressed batches, deterministic roots, idempotency keys, recent records, and dead-letter handling.
- Production oracle runtime with multi-source source health, quorum, stale/outlier rejection, confidence scoring, and source/stat APIs.
- AMM runtime with hardened constant-product, stable/weighted pool foundations, LP operations, slippage guards, and oracle deviation checks.
- Risk and liquidation engines for account health, margin, collateral, candidates, and liquidation planning.
- Perps and index fund engines for mark/index pricing, PnL, NAV, rebalance, mint, and redeem planning.
- WAL/recovery foundation with checkpoints, replay planning, malformed-record detection, and recovery reports.
- Production streaming hub for JSON/binary topic streams and recent replay records.
- Tile runtime with Crossbeam queues, supervisor snapshots, health metrics, CPU pinning hooks, and auto-tuning guardrails.
- Safe Cranelift JIT kernel scaffold with native fallback and exact-equivalence guardrails.
- Hardware-aware runtime planning for balanced, low-latency, high-throughput, market-maker, oracle-heavy, stream-heavy, and storage-heavy profiles.
- Performance primitives for interning, object pools, reusable vectors, fast counters, and arenas.

## Repository layout

```text
apps/
  gravityd/          # service binary
  gravitybench/      # benchmark/report binary

crates/
  adapters/          # optional exchange/feed adapters
  amm/               # AMM runtime
  api/               # HTTP/WebSocket API layer
  book/              # CLOB/orderbook engine
  config/            # config loading/types
  core/              # service coordination
  database/          # memory store, persistence boundaries, migrations
  hardware/          # hardware/profile planning
  index/             # index fund engine
  liquidator/        # liquidation runtime
  market/            # market feed/event model
  oracle/            # oracle runtime
  perf/              # performance primitives
  perps/             # perpetual futures engine
  risk/              # risk runtime
  settlement/        # settlement finalization boundary
  stream/            # stream hub
  tile/              # tile runtime + JIT scaffold
  types/             # shared financial types
  wal/               # WAL/recovery runtime
  wire/              # binary hot-path frames
  worker/            # worker context/orchestration

config/              # checked-in example configs
docs/                # architecture and component docs
scripts/             # build/test/bench/release helpers
runtime/             # local runtime output; ignored except .gitkeep files
tests/               # integration/property/replay/stress placeholders
```

## Quick start

### Windows

```bat
scripts\build.bat
scripts\test.bat
scripts\bench.bat
scripts\start.bat
```

### Linux/macOS

```bash
./scripts/build.sh
./scripts/test.sh
./scripts/bench.sh
./scripts/start.sh
```

Run the service directly:

```bash
cargo run -p gravityd
```

Run a short demo:

```bash
cargo run -p gravityd -- --demo-seconds=10
```

Run the benchmark binary:

```bash
cargo run -p gravitybench --release -- \
  --orders=100000 \
  --compressions=1000 \
  --wire-ops=100000 \
  --wire-batches=1000 \
  --wire-batch-size=1024 \
  --parallel-markets=4 \
  --parallel-orders=100000 \
  --tiles=4 \
  --tile-jobs=250000 \
  --json-out=runtime/reports/gravity-bench.json \
  --csv-out=runtime/reports/gravity-bench.csv \
  --md-out=runtime/reports/gravity-release-report.md
```

## Release gate

```bash
./scripts/release-gate.sh
```

```bat
scripts\release-gate.bat
```

The release gate runs formatting, debug/release builds, optional JIT feature build, tests, benchmarks, and report checks.

## API overview

Core routes include:

```text
GET  /live
GET  /ready
GET  /health
GET  /metrics
GET  /api
GET  /api/version
GET  /api/errors
GET  /api/routes
GET  /api/openapi
GET  /ops
GET  /database
GET  /hardware
GET  /tiles
GET  /streams
GET  /wal/recovery-report
GET  /oracle
GET  /book/{symbol}?depth=25
POST /orders
POST /orders/batch
POST /binary/orders
POST /binary/orders/batch
POST /amm/pools/{symbol}/quote
POST /risk/check
POST /liquidations/scan
```

See [`docs/API.md`](docs/API.md), [`docs/API-CONTRACT.md`](docs/API-CONTRACT.md), and [`docs/SDK-CONTRACT.md`](docs/SDK-CONTRACT.md).

## Configuration

Example configs live in [`config/`](config/). They are safe defaults for local development and should be reviewed before production deployment.

Important files:

```text
config/gravity.toml
config/markets.toml
config/oracle.toml
config/risk.toml
config/database.toml
config/wal.toml
config/tile-runtime.toml
config/hardware.toml
config/security.toml
config/performance-final.toml
```

## Documentation

Start here:

- [`docs/README.md`](docs/README.md)
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- [`docs/MAP.md`](docs/MAP.md)
- [`docs/ROADMAP.md`](docs/ROADMAP.md)
- [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)
- [`docs/SECURITY-CORRECTNESS.md`](docs/SECURITY-CORRECTNESS.md)

Release manifests are archived under [`docs/releases/`](docs/releases/).

## Safety model

Gravity must never auto-tune or JIT-modify:

- price-time priority
- fixed-point financial math semantics
- settlement correctness
- replay ordering
- risk thresholds without explicit policy changes
- oracle quorum/staleness safety rules

All JIT kernels require native fallback and exact-equivalence checks.

## License

MIT. See [`LICENSE`](LICENSE).
