# Gravity Architecture

Gravity is Ascension's DeFi service layer. It handles market data, oracle reports, orderbook/AMM/risk services, and settlement payload preparation outside Stargate's consensus/execution hot path.

## v0.3 modules

```text
api          Axum REST API
config       config loading and validation
core         runtime orchestration
market       adapters, normalized events, bounded async bus
database     memory/Postgres/Redis storage boundary
oracle       median/VWAP aggregation and report signing
settlement   async Stargate settlement shell
types        fixed-point finance and shared events
worker       feed and processor task orchestration
```

## Flow

```text
External feeds / Mock feeds
  -> MarketAdapter
  -> bounded Tokio channel
  -> FeedMonitor
  -> OracleEngine
  -> GravityStore
  -> SettlementClient
  -> Stargate/L3 later
```

## Rule

Gravity may compute, validate, cache, batch, and submit. Stargate remains the final state machine.
