# Gravity Parallel Execution

Gravity v1.5 improves batch execution by dispatching grouped order batches to all affected market workers before awaiting replies.

## Flow

```text
/orders/batch or /binary/orders/batch
  ↓
group orders by market symbol
  ↓
send one SubmitBatch command to each market worker
  ↓
workers execute independently on the Tokio runtime
  ↓
results are restored to caller order
```

This gives parallelism across independent markets while preserving deterministic single-writer mutation inside each market's own orderbook.

## Why this is safe

Each market worker owns its own CLOB instance. No two workers mutate the same book. Cross-market batches run in parallel without sharing book state.

## Metrics

The following counters help validate behavior:

```text
microbatches
microbatch_commands
parallel_batch_groups
worker_enqueue_failed
```

Worker metrics are available at:

```text
GET /workers
GET /metrics
```
