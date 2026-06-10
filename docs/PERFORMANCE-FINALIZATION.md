# Gravity v3.7.0 Performance Finalization

Gravity v3.7.0 adds the final optimization foundation before the production release gate.

## Goals

- Reduce hot-path allocations.
- Reuse buffers and object slots.
- Intern repeated market symbols and account identifiers.
- Keep snapshot and stream paths zero-copy where possible.
- Prepare batch database writes and fast fanout behavior.
- Keep all deterministic trading and settlement rules unchanged.

## New crate

`crates/perf` provides reusable optimization primitives:

- `Interner` for repeated symbols/accounts.
- `SlotPool<T>` for object reuse.
- `ReusableVec<T>` for allocation-stable temporary buffers.
- `FastCounterMap<K>` for single-pass aggregation.
- `PerfArena` for benchmark and runtime tuning experiments.

## Guardrails

The performance layer must not change:

- CLOB price-time priority.
- Fixed-point arithmetic.
- Settlement idempotency.
- Replay order.
- Risk policy thresholds.
- Oracle quorum rules.
- JIT/native equivalence.

## Benchmark phase

`gravitybench` includes a `perf-pool` phase that exercises interners, reusable vectors, and object pools.

## Release targets

| Area | Target |
|---|---:|
| Tile queue | 10M+ ops/sec |
| Binary wire | 5M+ ops/sec |
| Parallel dispatch | 3M+ ops/sec |
| Batch order intake | 2M+ ops/sec |
| Match path | 1M+ ops/sec |
| Snapshot reads | 1M+ ops/sec |
| Risk checks | 1M+ ops/sec |
| AMM quotes | 1M+ ops/sec |
