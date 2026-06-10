# Gravity Database

Gravity uses Postgres for durable history and Redis for hot snapshots/fanout.

## v0.3 modes

```text
memory
postgres
postgres-redis
```

The default is `memory` so local development does not require services.

## Migrations

Located in:

```text
crates/database/migrations/
```

Current migrations:

```text
0001-oracle.sql
0002-market-events.sql
0003-counters.sql
```

## Postgres responsibility

- oracle report history/latest state
- normalized market event archive
- future orders/fills/positions/risk/liquidations

## Redis responsibility

- latest oracle report cache
- future orderbook depth snapshots
- WebSocket fanout cache
- short-lived worker coordination
