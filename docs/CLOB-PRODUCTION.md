# Gravity v1.8 Production CLOB Engine

Gravity v1.8 moves the native orderbook from a simple matching foundation toward a production exchange-engine shape while preserving the single-writer per-market model.

## Added

- Market status controls: `Open`, `CancelOnly`, and `Halted`.
- Cancel-replace command path executed inside the owning market worker.
- Amend command path for safe quantity reduction without losing price-time priority.
- Self-trade prevention with default taker rejection.
- Market rule validation for tick size, lot size, and minimum quantity.
- Fee model fields on fills for maker/taker quote fees.
- New book event kinds for amendments and replacements.

## API

```text
POST /orders/{symbol}/{id}/amend
POST /orders/{symbol}/{id}/replace
POST /markets/{symbol}/status
```

## Determinism Rules

- Price changes require cancel-replace.
- Time-in-force changes require cancel-replace.
- Quantity amendment may only reduce remaining quantity.
- Market status changes do not alter existing book ordering.
- Self-trade prevention rejects the taker before creating a fill.
- Matching remains price-time priority.

## Performance Rules

- Amend/replace/status commands are routed to the same single-writer market worker.
- Snapshot cache invalidation remains local to the owning book.
- The order index remains active for faster cancel lookup.
- Settlement/fill behavior remains compatible with previous compressed settlement work.
