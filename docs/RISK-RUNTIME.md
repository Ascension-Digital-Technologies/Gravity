# Gravity v2.2 Risk Runtime

Gravity v2.2 adds the first production-oriented portfolio risk engine.

## Scope

- Account health snapshots
- Collateral valuation with per-asset haircut/collateral factor
- Position notional calculation from oracle/mark prices
- Initial and maintenance margin requirements
- Equity, free collateral, health factor, and leverage outputs
- Oracle dependency tracking
- Risk events and risk stats
- API routes and benchmark phase

## API

```text
POST /risk/check
GET  /risk/accounts/{account}
GET  /risk/events?limit=100
GET  /risk/stats
```

## Example check

```json
{
  "account": "acct-1",
  "collaterals": [
    { "asset": "USDx", "quantity": "10000", "price": "1", "collateral_factor_bps": 9500 }
  ],
  "positions": [
    { "symbol": "BTC-USDx", "quantity": "0.1", "mark_price": "100000", "side": "long" }
  ],
  "debt_value": "1000",
  "maintenance_margin_bps": 1000,
  "initial_margin_bps": 2000
}
```

## Status model

```text
Healthy
Watch
MarginCall
Liquidatable
```

Risk remains deterministic and fixed-point only. It does not finalize liquidations by itself; it prepares data for the upcoming liquidation engine and settlement boundary.
