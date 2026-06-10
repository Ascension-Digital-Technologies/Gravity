# Gravity v2.1 AMM Runtime

Gravity v2.1 adds the native AMM runtime beside the production CLOB and oracle layers.

## Supported pool kinds

- Constant product
- Stable simulation foundation
- Weighted pool foundation

## Runtime behavior

- Fixed-point math only
- Quote and swap simulation
- LP supply accounting hooks
- Pool snapshots
- Pool events
- Settlement hints for Stargate/L3 payload builders

Gravity plans and simulates AMM state transitions. Stargate/L3 remains the final source of truth.

## API

```text
GET  /amm/pools
POST /amm/pools
GET  /amm/pools/{symbol}
POST /amm/pools/{symbol}/quote
POST /amm/pools/{symbol}/swap
POST /amm/pools/{symbol}/liquidity
GET  /amm/events?limit=100
```

## Performance target

`amm-quote` target: 1,000,000 quotes/sec on strong hardware.
