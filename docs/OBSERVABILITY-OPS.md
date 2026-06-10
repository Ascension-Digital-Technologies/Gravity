# Gravity v3.6.0 Observability + Ops Console

Gravity v3.6.0 adds a read-only operations layer for production visibility. The goal is to make the runtime easy to inspect without touching deterministic market execution, settlement correctness, risk policy, or replay ordering.

## New endpoints

```text
GET /ops
GET /ops/health
GET /ops/startup
GET /ops/queues
GET /ops/latency
GET /ops/services
GET /ops/performance
```

## What the ops layer reports

```text
service version and uptime
startup state
hardware profile and placement plan
database and migration status
WAL recovery status
stream status
tile supervisor status
worker queue depth and latency
storage backpressure
settlement status
risk/liquidation/oracle/perps/index status
```

## Guardrails

The ops layer is intentionally read-only. It can observe queue pressure, latency, and service health, but it must not mutate:

```text
price-time priority
fixed-point financial math
settlement finality rules
risk thresholds
liquidation policy
WAL replay ordering
JIT/native equivalence
```

## Recommended production flow

```text
1. Start Gravity.
2. Check GET /ops/startup.
3. Check GET /ops/health.
4. Watch GET /ops/queues and GET /ops/latency under load.
5. Use GET /ops/performance after benchmark/profile runs.
6. Keep GET /metrics available for machine-readable monitoring.
```

## Report targets

The config file `config/ops.toml` defines report paths under `runtime/reports/` for startup, health, performance, and service summaries. A later release can write full Markdown reports on startup and release-gate execution.
