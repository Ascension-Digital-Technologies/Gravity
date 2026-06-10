# Exchange Adapters

Gravity exchange adapters are optional feed connectors. They provide external market truth for oracle, risk, liquidation, index, and synthetic systems, but they are not part of Stargate consensus/execution and they are not required for Gravity to boot.

## Design rules

- Keep adapters outside the DeFi core.
- Normalize every venue into `MarketEvent`.
- Reject duplicate or old sequences.
- Track gaps, staleness, and venue health.
- Use fixed-point values only.
- Never let a single venue define final oracle truth.
- Treat live exchange APIs as untrusted inputs.

## Current venues

- Binance
- Coinbase Exchange
- Kraken

## Current mode

The v0.4 implementation runs adapters in replay mode. Replay mode emits venue-shaped payloads, passes them through the same normalizers, and publishes real Gravity `MarketEvent` values. This allows local testing without relying on public WebSocket availability.

## Live mode requirements

Before turning on live sockets, add:

1. reconnect backoff with jitter
2. venue-specific rate limits
3. heartbeat/pong monitoring
4. subscription ack verification
5. sequence gap repair policy
6. source trust scoring
7. malformed payload quarantine
8. metrics off the hot path
