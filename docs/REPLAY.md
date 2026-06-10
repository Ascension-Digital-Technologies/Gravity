# Gravity Replay Plan

Gravity replay is designed around normalized events and audit records.

## Sources

1. normalized market events
2. book events
3. oracle reports
4. settlement payloads
5. audit records

## Goal

A future replay binary should rebuild:

- oracle state
- orderbook snapshots
- book event sequences
- settlement batches
- risk/liquidation inputs

The v1.0 package adds the audit and schema foundation needed for replay determinism. Full replay execution belongs in a later pass once AMM/risk/perps modules are present.
