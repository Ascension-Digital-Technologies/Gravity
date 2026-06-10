# Gravity Index Fund Runtime

Gravity v2.7 adds the native index-fund engine for Ascension products such as crypto basket funds and the protocol's "crypto Dow Jones" direction.

## Capabilities

- basket definitions with target weights
- fixed-point NAV calculation
- NAV-per-unit calculation
- oracle dependency maps
- rebalance planning with drift thresholds
- mint planning
- redeem planning
- management fee modeling
- settlement hint output
- index events and stats

## API

```text
GET  /index/products
POST /index/products
GET  /index/products/{id}
POST /index/products/{id}/nav
POST /index/products/{id}/rebalance
POST /index/products/{id}/mint
POST /index/products/{id}/redeem
GET  /index/events?limit=100
GET  /index/stats
```

## Performance rules

The runtime uses fixed-point math only. Gravity plans NAV/rebalance/mint/redeem behavior; Stargate/L3 remains final settlement truth.
