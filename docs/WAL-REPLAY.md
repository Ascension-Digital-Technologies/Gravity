# Gravity v2.4 WAL + Replay Runtime

Gravity v2.4 adds a durable write-ahead log foundation for accepted runtime events before slower persistence layers complete.

## Streams

- `orders.wal`
- `fills.wal`
- `settlement.wal`
- `oracle.wal`
- `amm.wal`
- `risk.wal`
- `liquidation.wal`
- `generic.wal`
- `checkpoints.jsonl`

## Runtime APIs

```text
GET  /wal
GET  /wal/recent?limit=100
POST /wal/checkpoint
GET  /wal/replay-plan
```

## Design rules

- WAL append is bounded by a recent in-memory ring plus append-only files under `runtime/wal/`.
- WAL records are JSON-lines for easy inspection and recovery tooling.
- Existing persistence queues still handle hot/cold DB writes.
- WAL replay is plan-first in v2.4: startup replay and full state rebuild are staged for the next durability pass.
- Settlement idempotency keys remain the safety boundary for replayed settlement batches.

## Recovery sequence target

```text
load latest checkpoint
scan WAL streams after checkpoint
rebuild volatile order/oracle/AMM/risk state
reconcile settlement receipts
resubmit unfinalized batches by idempotency key
start API once replay reaches a clean boundary
```
