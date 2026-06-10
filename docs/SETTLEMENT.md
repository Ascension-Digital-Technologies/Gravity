# Settlement

Gravity settlement is an async shell in v0.3.

It creates deterministic payloads and uses a local idempotency guard so the same payload reference is only accepted once.

## Current payloads

```text
UpdateOracle
```

## Future payloads

```text
PlaceOrder
CancelOrder
SettleTrade
Swap
AddLiquidity
RemoveLiquidity
Liquidate
RebalanceIndex
```

## Rule

Gravity does not commit chain state. It submits payloads to Stargate/L3 and waits for receipts.
