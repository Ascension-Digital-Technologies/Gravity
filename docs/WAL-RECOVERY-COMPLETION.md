# Gravity v3.2.0 WAL Replay + Recovery Completion

Gravity v3.2.0 upgrades the WAL layer from append-only recording into a recovery-aware runtime.

## Added

- checkpoint loading
- stream file scanning
- malformed JSONL detection
- sequence regression detection
- latest checkpoint reporting
- dry-run replay endpoint
- recovery report endpoint
- recovery actions for startup tooling

## API

```text
GET  /wal/checkpoints?limit=100
GET  /wal/recovery-report
POST /wal/replay-run
```

`/wal/replay-run` is intentionally a dry run in this release. It verifies persisted WAL files and returns the exact recovery action plan without mutating runtime state.

## Verdicts

```text
Healthy  - all scanned streams are readable and ordered
Degraded - some expected streams are missing or empty
Corrupt  - malformed WAL lines or sequence regressions were found
```

## Startup recovery target

```text
load config
validate migrations
load latest checkpoint
scan WAL streams
rebuild volatile runtime state
reconcile settlement batches by idempotency key
requeue dead letters for operator-approved retry
start APIs/workers
mark ready
```

## Guardrails

Recovery must never change deterministic financial rules. It can rebuild state, but it cannot rewrite order priority, fixed-point math, settlement IDs, or risk policy.
