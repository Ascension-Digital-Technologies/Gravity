# Gravity Runtime

Gravity v0.8.0 focuses on live runtime behavior rather than more scaffolding.

## Runtime streams

WebSocket streams are now event-driven through in-process broadcast channels:

- `/ws/book/{symbol}` sends an initial depth snapshot, then live book events.
- `/ws/oracle` sends an initial oracle snapshot, then live oracle reports.

Each stream sends a heartbeat every 15 seconds so clients can detect stalled connections.

## Storage behavior

Gravity still keeps the in-memory store as the hot runtime source for fast local reads.
When `storage_mode = "postgres"`, oracle reports, market events, order results, fills, and book events are also written to Postgres.
When `storage_mode = "postgres-redis"`, latest oracle reports and depth snapshots are cached in Redis, and book events are also appended to Redis streams.

## Benchmark reports

`scripts/bench.bat` and `scripts/bench.sh` now write benchmark reports to:

```text
runtime/reports/clob-bench.json
runtime/reports/clob-bench.csv
```

The benchmark remains a standalone binary so it does not depend on unstable Rust benchmark APIs.


## v1.0 Market Worker Runtime

Gravity v1.0 adds per-market single-writer CLOB workers, bounded queues, cached depth snapshots, fixed-size event rings, worker metrics, and batch order intake.
