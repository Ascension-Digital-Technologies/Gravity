# Settlement Compression

Gravity v1.3 adds compressed settlement batches for CLOB fills.

Instead of submitting one settlement payload per fill, Gravity can compress a group of fills into one `SettleCompressedTrades` payload. The payload contains:

- batch id
- symbol
- fills root
- fill ids
- net account deltas
- compression report
- created timestamp

## Why

The goal is to reduce downstream Stargate/L3 load during high-volume matching periods.

```text
many fills -> net deltas -> one compressed settlement payload
```

## Current Delta Model

The current model groups by account and symbol and records:

- fill count
- bought quantity raw
- sold quantity raw
- quote notional raw

This keeps the first version deterministic and auditable while leaving room for richer fee, rebate, and asset-leg accounting later.
