# Orderbook

Gravity's CLOB uses a native Rust orderbook with fixed-point prices and quantities.

## v0.6 capabilities

- price-time priority
- limit and market orders
- GTC, IOC, FOK, and post-only handling
- cancel path
- fill generation
- depth snapshots
- memory depth cache
- event history for stream fanout
- SQLx order/fill persistence boundary
- Redis depth cache hook

## Performance direction

Each market should eventually run as a single-writer async worker. Reads should use cached snapshots, not hot locks. Settlement batches should be emitted separately so Stargate remains final truth.


## v1.0 Market Worker Runtime

Gravity v1.0 adds per-market single-writer CLOB workers, bounded queues, cached depth snapshots, fixed-size event rings, worker metrics, and batch order intake.
