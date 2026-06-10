# Oracle

Gravity v0.3 supports median and VWAP-style aggregation over normalized market events.

## Safety rules

- fixed-point prices only
- minimum source count
- stale source rejection
- deviation filtering
- deterministic payload hash
- optional local signing scaffold

## Current output

`OracleReport` includes:

```text
symbol
price
confidence_bps
sources
method
timestamp_ms
key_id
payload_hash
signature
```

## Upcoming hardening

- real signer integration
- threshold signatures
- per-source trust weighting
- cross-source latency scoring
- oracle replay tests


## v0.4 adapter update

See `EXCHANGE-ADAPTERS.md` for the optional Binance/Coinbase/Kraken adapter layer.
