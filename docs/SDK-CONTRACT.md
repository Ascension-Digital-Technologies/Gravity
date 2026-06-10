# Gravity SDK Contract

Gravity SDK clients should implement stable wrappers around the v3.5.0 API contract.

## Required SDK modules

- `Client` — REST calls, retries, request IDs, base URL
- `Orders` — CLOB order submit, batch submit, amend, replace, cancel
- `Binary` — binary order/batch intake using Gravity Wire frames
- `Oracle` — latest reports, source health, stats
- `Amm` — pools, quote, swap plan, liquidity add/remove
- `Risk` — account checks, risk stats/events
- `Liquidations` — scan, candidates, liquidation plans
- `Perps` — markets, positions, funding
- `Index` — products, NAV, rebalance, mint/redeem plans
- `Streams` — JSON and binary stream subscribers
- `Ops` — health, metrics, database, WAL, tiles, hardware

## Request policy

SDKs should send:

- `x-request-id` for every request
- `idempotency-key` for every mutating request that can be retried

## Retry policy

Safe retry candidates:

- read endpoints
- idempotent mutating endpoints with idempotency keys
- settlement/dead-letter retry calls when explicitly requested

Do not blindly retry non-idempotent order or swap submissions without an idempotency key.

## Binary contract

High-speed internal clients should prefer `/binary/orders` and `/binary/orders/batch` for market-maker/bot paths. Public dashboards should prefer JSON APIs and streams.
