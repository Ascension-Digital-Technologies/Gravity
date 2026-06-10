
## v2.1.0 Native AMM Runtime

Added Gravity AMM runtime with pool creation, quotes, swaps, liquidity hooks, AMM events, settlement hints, and `amm-quote` benchmarks.

# Gravity Performance Model

Gravity v1.0 moves the CLOB path to a per-market actor runtime. Each symbol is lazily assigned a single writer that owns its orderbook, drains a bounded command queue, refreshes cached depth snapshots, appends to fixed-size event rings, and publishes book events to WebSocket subscribers.

## Hot Path

```text
API/order intake -> bounded market queue -> single market worker -> OrderBook mutation -> cached depth/event ring -> async persistence/cache hooks
```

## Why this is faster

- no shared global orderbook mutex on the write path
- deterministic command ordering per market
- batch drain loop reduces scheduler overhead under load
- reads use cached snapshots when possible
- event history is ring-buffered with a fixed cap
- worker metrics are off the critical mutation path

## New endpoints

```text
GET  /workers
POST /orders/batch
GET  /metrics
```

## Upcoming performance work

1. true grouped batch commands per market
2. binary internal worker frames
3. hot/cold database writer queue
4. settlement batch compressor
5. core-aware worker placement


## v1.2 Hot Path Performance

Gravity v1.2 adds grouped batch submission per market. Batch API calls are grouped by symbol and sent as a single worker command per market instead of one channel send per order. Market workers refresh cached depth snapshots once after each batch, reducing repeated snapshot work under load. Common book depths are cached at 10, 25, 50, and 100 levels.

## v1.3 Settlement + Persistence Performance

Gravity v1.3 adds two important runtime protections:

1. compressed settlement batches so large fill bursts can be reduced into net account deltas before submission to Stargate/L3;
2. bounded hot/cold persistence queues so database pressure cannot create unbounded memory growth on the market worker path.

Market workers now use fail-fast enqueue behavior when the bounded queue is full. This protects latency during overload and makes backpressure visible to API callers instead of silently accumulating work.


## v1.4.0 Wire Layer

Added `crates/wire` for compact binary hot-path frames and benchmark coverage.


## v1.5 Throughput Path

The primary v1.5 performance changes are binary order intake, grouped batch dispatch, parallel market execution, and expanded benchmarks. Independent market groups are sent to workers before replies are awaited, allowing multiple CLOB workers to execute at the same time while each book remains single-writer deterministic.


## v1.6 Tile Runtime Additions

Gravity now includes a dedicated tile runtime layer: crossbeam queues, tile roles, CPU pinning, adaptive batch tuning, and a Cranelift JIT feature boundary for future deterministic hot kernels.

## v1.7 Hot Path Acceleration

The v1.7 pass targets measured bottlenecks from v1.6.2.

- **Cancel path:** CLOB now keeps an order-id to side/price location index so cancellation routes directly to the correct price level instead of scanning both books.
- **Snapshot path:** orderbook snapshots are cached per requested depth and invalidated only when book state changes.
- **Compression path:** settlement compression now avoids giant joined fill-id strings and uses a single-pass aggregation map with compact fill-id retention for large batches.
- **Benchmark accuracy:** benchmark reports now include nanosecond percentile fields in addition to the existing microsecond fields.



## v1.9.0 Validation Gate

See `docs/VALIDATION-GATE.md` for the release validation flow, benchmark target verdicts, and report checks before the v1.8 Production CLOB pass.


## v2.0.0 Production Oracle Runtime

Added production oracle source health, confidence scoring, expanded aggregation modes, signed report hardening, oracle stats/source APIs, migration shell, and benchmark coverage.


## v2.2.0 Risk Runtime

Added `crates/risk`, risk APIs, risk benchmark coverage, and risk persistence migration shells.


## Gravity v2.3.0 Liquidation Runtime

Added liquidation candidate scanning, partial/full liquidation planning, priority scoring, API routes, benchmark coverage, config, migration shell, and docs.


## AMM Hardening Benchmark

`gravitybench` includes `amm-hardening` to cover stable/weighted quote paths and LP removal behavior.


## v2.6.0 - Perps Engine Foundation

- Added `gravity-perps` crate.
- Added perpetual futures markets, position opening, funding updates, mark/index pricing, PnL/equity/margin calculations, liquidation price hints, API routes, benchmark coverage, config, migration shell, and docs.



## v2.7.0 Index Fund Engine

Added index product runtime, NAV/rebalance/mint/redeem planners, API routes, migration shell, and benchmark coverage.

## Tile Supervisor Benchmark

`gravitybench` now includes `tile-supervisor` to measure supervised tile ping/health throughput.

## v3.0.0 Safe JIT Kernel Phase

The benchmark includes `jit-kernels`, which checks native/accelerated equivalence while measuring deterministic math kernels. This prepares Gravity for future Cranelift-backed acceleration without risking exchange-engine ordering correctness.


## v3.1.0 Hardware-Aware Runtime

Adds machine profiling, placement plans, hardware profiles, pinning hints, and hardware-placement benchmark coverage.
