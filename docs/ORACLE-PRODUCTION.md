# Gravity v2.0.0 Production Oracle Runtime

Gravity's oracle runtime now tracks venue/source health separately from final reports.

## Features

- Multi-source source map per symbol.
- Median, VWAP, TWAP, EWMA, and median-vwap method names.
- Stale-source rejection.
- Outlier classification by basis-point deviation from the accepted report.
- Confidence scoring.
- Signed deterministic oracle reports.
- Source health API and stats API.
- Oracle source persistence/audit migration shell.

## APIs

```text
GET /oracle
GET /oracle/{symbol}
GET /oracle/sources
GET /oracle/stats
GET /ws/oracle
```

## Safety rules

The oracle never finalizes chain state. It only emits signed reports and settlement payloads. Stargate/L3 remains final truth.

A report is withheld when source quorum is missing, sources are stale, outliers leave insufficient accepted sources, or confidence falls below the configured threshold.
