# Gravity v3.4.0 Production Database Integration

Gravity v3.4.0 promotes the storage layer from migration shells and boundaries into a production-oriented integration surface.

## Storage modes

```text
memory
postgres
postgres-redis
```

Memory mode remains the fastest local/dev mode. Postgres mode enables durable SQLx-backed writes. Postgres-Redis mode adds hot cache and stream fanout support.

## New operational endpoints

```text
GET /database
GET /database/migrations
GET /database/backpressure
```

These endpoints report Postgres health, Redis health, migration status, and hot/cold persistence queue pressure.

## Production additions

- Migration validation against required Gravity migration files.
- Redis PING health check.
- Generic `persistence_records` table for hot/cold queue durability.
- Storage health check table shell.
- Backpressure report with queue pressure in basis points.
- Database state included in `/metrics`.

## Guardrail

Storage must never sit in the CLOB hot path. Gravity writes into memory/rings first, then durable writes run through bounded queues and repository methods.
