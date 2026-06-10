# Gravity Tile Runtime

Gravity v1.6 adds a tile-based runtime layer for throughput, low latency, and hardware-aware execution.

Tiles are isolated execution units around hot Gravity services:

- `Ingress` — external/API intake
- `Intake` — binary/JSON decode and validation
- `Match` — per-market CLOB execution
- `Oracle` — oracle aggregation
- `Risk` — risk checks and circuit breakers
- `Settlement` — settlement compression/submission
- `Storage` — hot/cold persistence flushing
- `Stream` — WebSocket/API fanout

## Design

Each tile owns a bounded crossbeam queue and drains commands in microbatches. This avoids unbounded memory growth and gives Gravity a direct place to measure queue depth, latency, processed commands, and rejected commands.

```text
API / binary intake
  -> intake tile
  -> match tile per hot market
  -> settlement tile
  -> storage / stream tiles
```

## CPU pinning

Tiles can request a logical CPU core. Pinning is best-effort and safe: if the operating system does not expose core affinity, Gravity continues normally.

## Auto-tuning

Adaptive tiles change batch size based on queue pressure:

- high pressure increases batch size
- low pressure reduces batch size
- capacity stays bounded

This keeps latency low during quiet periods and improves throughput under load.

## Cranelift JIT

Cranelift support is feature-gated with `cranelift-jit`. The default runtime registers deterministic kernel descriptors only. Future safe JIT targets include:

- risk math kernels
- settlement netting kernels
- oracle aggregation kernels
- binary decode/validation kernels

The CLOB correctness path should remain pure Rust until benchmarks prove a JIT kernel is worth installing.
