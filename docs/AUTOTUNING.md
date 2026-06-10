# Gravity Auto-Tuning Tiles

Auto-tuning lets Gravity adjust tile behavior without changing deterministic execution results.

## Tunable values

- microbatch size
- queue capacity per tile
- spin/sleep behavior
- worker placement policy
- hot market grouping

## Guardrails

- never changes order priority inside a market
- never changes fixed-point math semantics
- never bypasses settlement/audit/replay guards
- all tuning decisions are metrics-driven and reversible

## Metrics

Tiles expose:

- accepted commands
- rejected commands
- processed commands
- queue depth
- batch size
- average latency
- max latency
- CPU pinning status
