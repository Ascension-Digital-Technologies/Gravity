# Gravity Audit Trail

Gravity v1.0 adds a deterministic in-memory audit trail for the production service path.

## Record types

- `market_event` — normalized market data accepted into Gravity
- `oracle` — oracle reports stored by the service
- `order` — order submissions and resulting order state
- `cancel` — successful cancellation events

Each record contains:

- stable ID
- kind
- target symbol/account
- sequence
- timestamp
- payload hash
- human-readable message

The audit trail is intentionally lightweight and off the hot path. Postgres schema support is included through `0006-audit.sql` so durable writes can be enabled without changing API contracts.

## API

```text
GET /audit?limit=100
```

Returns the latest audit records in chronological order.
