
## v2.1.0 Native AMM Runtime

Added Gravity AMM runtime with pool creation, quotes, swaps, liquidity hooks, AMM events, settlement hints, and `amm-quote` benchmarks.

# Gravity API

## Probes

- `GET /live` returns process liveness and uptime.
- `GET /ready` validates storage readiness.
- `GET /health` returns service health.
- `GET /metrics` returns current counters.

## Oracle

- `GET /oracle`
- `GET /oracle/{symbol}`
- `GET /ws/oracle`

## CLOB

- `GET /book/{symbol}?depth=25`
- `GET /book/{symbol}/events?limit=100`
- `GET /stream/book/{symbol}?limit=100`
- `GET /ws/book/{symbol}`
- `POST /orders`
- `POST /orders/{symbol}/{id}/cancel`

Example order body:

```json
{
  "account": "demo-maker",
  "symbol": "BTC-USDx",
  "side": "sell",
  "kind": "limit",
  "tif": "gtc",
  "price": "100000.25",
  "quantity": "0.5",
  "client_id": "demo-1"
}
```


## v1.0 endpoints

```text
GET /audit?limit=100
GET /events/book?limit=100
```

`/audit` returns recent deterministic audit records. `/events/book` returns recent book events across all symbols.


## v1.0 Market Worker Runtime

Gravity v1.0 adds per-market single-writer CLOB workers, bounded queues, cached depth snapshots, fixed-size event rings, worker metrics, and batch order intake.

## v1.3 Persistence Endpoints

```text
GET /persistence
GET /persistence/recent?limit=100
```

`/persistence` returns the bounded hot/cold persistence queue depth, capacity, and dropped-record count.

`/persistence/recent` returns recent queued persistence records for operational inspection.


## Binary Intake

```text
POST /binary/orders
POST /binary/orders/batch
```

These endpoints accept `gravity-wire` binary frames and return JSON result envelopes. They are intended for internal services, market makers, SDK backends, and future high-speed clients.

## Production CLOB Endpoints

```text
POST /orders/{symbol}/{id}/amend
POST /orders/{symbol}/{id}/replace
POST /markets/{symbol}/status
```

## Settlement Finalization APIs

```text
GET  /settlement
GET  /settlement/recent?limit=100
GET  /settlement/dead-letter?limit=100
POST /settlement/dead-letter/retry?limit=100
```

These endpoints expose the local settlement finalizer boundary introduced in v1.9.0. The CLOB engine submits compressed fill batches into this boundary after matched trades. Stargate/L3 integration can replace the local finalizer later without changing orderbook execution.


## AMM Hardening Routes

- `POST /amm/pools/{symbol}/liquidity/remove`
- `POST /amm/pools/{symbol}/oracle-guard`


## v3.6.0 Ops Routes

```text
GET /ops
GET /ops/health
GET /ops/startup
GET /ops/queues
GET /ops/latency
GET /ops/services
GET /ops/performance
```
