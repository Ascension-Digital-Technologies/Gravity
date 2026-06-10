# Gravity v2.9 Tile Runtime Completion

Gravity v2.9 promotes the tile layer from a benchmark scaffold into a first-class runtime surface.

## Added

- Expanded tile kinds for order routing, AMM, liquidation, perps, index, audit, and metrics.
- `TileSupervisor` with runtime snapshots, health verdicts, queue pressure metrics, ping checks, and controlled restart-all support.
- Per-tile health states: `Healthy`, `Degraded`, and `Unhealthy`.
- Queue pressure basis-point reporting for auto-tuning and operations dashboards.
- `/tiles`, `/tiles/health`, `/tiles/ping`, and `/tiles/restart` API routes.
- Benchmark phase: `tile-supervisor`.
- `config/tile-runtime.toml` for production pinning and health settings.

## Guardrails

Auto-tuning and supervisor restarts never change price-time priority, fixed-point math, settlement correctness, or replay order.
