# Gravity v3.5.0 API Contract

Gravity v3.5.0 locks the public and internal interface shape for SDKs, dashboards, bots, market makers, indexers, and Ascension internal services.

## Contract endpoints

- `GET /api` — contract summary
- `GET /api/version` — runtime/API version metadata
- `GET /api/errors` — standard error catalog
- `GET /api/routes` — route inventory
- `GET /api/openapi` — generated OpenAPI starter document

## Standard response envelope

All new SDK-facing APIs should converge toward this shape:

```json
{
  "ok": true,
  "request_id": "req_...",
  "data": {},
  "error": null
}
```

Failure shape:

```json
{
  "ok": false,
  "request_id": "req_...",
  "data": null,
  "error": {
    "code": "invalid_request",
    "message": "human-readable failure",
    "details": {}
  }
}
```

## Headers

- `x-request-id`: client supplied request correlation ID. Gravity may generate one later when absent.
- `idempotency-key`: required for production order, settlement, mint/redeem, liquidation, and batch-submission flows once auth/rate-limit gates are enabled.

## Pagination

Existing endpoints use `limit`. Cursor pagination is reserved and should use:

```json
{
  "limit": 100,
  "cursor": "opaque_cursor",
  "next_cursor": "opaque_cursor_or_null"
}
```

## Public vs admin routes

Public/service routes include market data, books, oracle, AMM, risk, liquidations, perps, index products, and streams.

Admin/ops routes include tiles, hardware plans, database status, WAL recovery, settlement dead-letter replay, and future auth-protected controls.

## Compatibility rule

Breaking response or request changes require an API version bump. Internal route additions can ship without breaking existing SDKs.


## v3.6.0 Ops Contract

The `/ops` endpoints are read-only production observability routes for runtime dashboards and automated health monitors.
