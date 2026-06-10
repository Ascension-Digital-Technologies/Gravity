# Market Workers

Each active market now has a lazy worker. The first order or snapshot request for a symbol starts the worker. The worker owns the CLOB instance for that symbol and accepts commands through a bounded Tokio channel.

## Commands

```text
Submit order
Cancel order
Snapshot request
```

## Guarantees

- one writer mutates the book
- command order is deterministic per market
- queues apply backpressure instead of unlimited memory growth
- depth snapshots and event rings are shared for fast readers

## Metrics

Worker stats include processed command count, submitted orders, cancels, snapshots, fills, rejects, maximum drained batch size, last command latency, latest sequence, and queue capacity.
