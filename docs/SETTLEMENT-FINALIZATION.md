# Gravity Settlement Finalization

Gravity v1.9.0 turns settlement from a compressed-fill payload scaffold into a deterministic finalization boundary for Stargate/L3.

## Goals

- Compress CLOB fills into deterministic net settlement deltas.
- Produce stable idempotency keys and payload roots.
- Preserve audit roots and sequence windows for replay.
- Track accepted, duplicate, finalized, failed, and dead-letter records.
- Keep retry/dead-letter queues off the matching hot path.

## Flow

```text
CLOB fills
  -> compressed settlement batch
  -> Stargate instruction boundary
  -> local idempotency guard
  -> receipt/finalization record
  -> recent settlement ring / dead-letter queue
```

## API

```text
GET  /settlement
GET  /settlement/recent?limit=100
GET  /settlement/dead-letter?limit=100
POST /settlement/dead-letter/retry?limit=100
```

## Determinism

The settlement batch records:

- symbol
- fills root
- audit root
- sequence window
- net account deltas
- compact fill id list
- idempotency key

The local finalizer is still a Stargate boundary placeholder. It is intentionally isolated behind `SettlementClient`, so the later Stargate/L3 submitter can replace the local finalizer without changing the CLOB engine.
