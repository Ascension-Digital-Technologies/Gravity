# Settlement Batching

Gravity v1.0 adds a batch wrapper around settlement payloads.

## Why

CLOB fills can produce many small settlement instructions. Batching lets Gravity group them before submitting to Stargate/L3 while keeping the local idempotency guard intact.

## Current behavior

- Oracle reports still submit as single payloads.
- Trade fills can be converted into `SettleTrade` payloads and submitted as a `SettlementBatch`.
- Duplicate idempotency keys are ignored locally.
- The returned `SettlementBatchReceipt` reports accepted and duplicate counts.

This remains a local shell until Stargate's final settlement API is locked.
