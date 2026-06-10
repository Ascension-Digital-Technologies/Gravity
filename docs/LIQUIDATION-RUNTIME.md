# Gravity Liquidation Runtime

Gravity v2.3.0 adds the liquidation runtime. It consumes risk snapshots, ranks unhealthy accounts, prepares partial/full liquidation plans, and exposes candidates/events/stats over the API.

## Flow

```text
risk snapshots
  -> liquidation scanner
  -> candidate priority ranking
  -> partial/full plan
  -> settlement hint
  -> Stargate/L3 finalization boundary later
```

## API

```text
POST /liquidations/scan
GET  /liquidations/candidates?limit=100
POST /liquidations/accounts/{account}/plan
GET  /liquidations/events?limit=100
GET  /liquidations/stats
```

## Performance Rules

- Candidate scanning uses deterministic risk snapshots.
- Priority is based on health gap plus deficit value.
- Plans are fixed-point only.
- Gravity prepares liquidation plans; Stargate/L3 remains final truth.
