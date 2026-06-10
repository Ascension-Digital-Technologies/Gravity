# Gravity v2.5.0 AMM Production Hardening

This release turns the initial AMM runtime into a safer production-facing AMM layer.

## Added

- Constant-product pool path preserved.
- Stable pool foundation strengthened with amplification-aware imbalance penalty.
- Weighted pool foundation strengthened with base/quote weight validation.
- LP remove-liquidity path with minimum base/quote output guards.
- Swap max-price-impact guard.
- Oracle deviation guard endpoint for AMM/CLOB/oracle safety checks.
- Expanded pool snapshots with weights, amplification, and max impact policy.
- AMM hardening benchmark phase.

## API

```text
POST /amm/pools/{symbol}/liquidity/remove
POST /amm/pools/{symbol}/oracle-guard
```

Remove liquidity body:

```json
{
  "lp": "10",
  "min_base": "0.1",
  "min_quote": "100"
}
```

Oracle guard body:

```json
{
  "oracle_price": "100000",
  "max_deviation_bps": 250
}
```

## Determinism

All AMM math remains fixed-point. The AMM still plans and simulates; Stargate/L3 remains final settlement truth.

## Upcoming

The deeper production AMM work should add concentrated liquidity, curve-specific invariant tests, AMM/CLOB routing, and settlement finalization for swap/liquidity records.
