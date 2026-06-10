# Gravity v3.1 Hardware-Aware Runtime

Gravity v3.1 adds a portable hardware-awareness layer for production placement planning.

## Goals

- detect operating system, architecture, logical cores, and CPU pinning availability
- generate runtime placement plans for match, stream, storage, settlement, oracle/risk, and ingress tiles
- keep storage and stream work away from hot market matching cores where possible
- expose placement information through APIs and benchmark reports
- preserve deterministic order rules, fixed-point math, settlement correctness, and replay order

## Runtime profiles

- balanced
- low-latency
- high-throughput
- market-maker
- oracle-heavy
- stream-heavy
- storage-heavy

## APIs

```text
GET /hardware
GET /hardware/plan?profile=low-latency
GET /hardware/simulate?profile=high-throughput
```

## Guardrails

The hardware runtime may tune batch sizes, queue hints, and placement hints. It must never tune price-time priority, fixed-point financial math, settlement correctness, risk thresholds without policy approval, or replay ordering.
