# Gravity v1.9.0 CLOB Correctness Suite

Gravity v1.9.0 focuses on locking the native Ascension CLOB before the settlement/oracle/risk layers become deeper.

## Covered behavior

The suite now covers the highest-risk exchange-engine rules:

- price-time FIFO within a single price level
- market orders never resting when liquidity is unavailable
- IOC partial fills expiring remaining quantity
- FOK full-fill execution and FOK rejection behavior
- post-only crossing rejection and non-crossing rest behavior
- self-trade prevention using taker rejection
- amend reduction without priority loss
- amend rejection for quantity increases
- amend rejection for price/time-in-force changes
- missing cancel as a no-op
- snapshot cache invalidation after cancel
- missing replace as no-op without submitting a replacement
- halted market replace rejection
- CancelOnly mode allowing cancels but blocking new orders
- Halted mode blocking cancels
- tick-size, lot-size, and minimum quantity validation
- maker/taker fee recording on fills

## Invariants

The CLOB must preserve these invariants before production settlement expands:

```text
1. A market order cannot rest on the book.
2. IOC remaining quantity cannot rest on the book.
3. FOK cannot mutate the book unless the full quantity is fillable.
4. FIFO order is preserved for resting orders at the same price.
5. Cancel cannot remove the wrong order.
6. Amend cannot increase remaining quantity.
7. Amend cannot change price or time-in-force; cancel-replace is required.
8. Post-only orders cannot take liquidity.
9. Self-trade prevention rejects the taker before creating a fill.
10. Snapshots must reflect the latest book sequence after mutation.
11. Fee fields must be recorded deterministically on fills.
```

## Why this matters

Settlement, risk, liquidations, perps, synthetics, and index products depend on accurate fills. The CLOB is the source of matched-trade intent; Stargate/L3 remains final settlement truth.

v1.9.0 is intentionally test-heavy and feature-light so the exchange engine is stable before Gravity v1.9 production settlement.
