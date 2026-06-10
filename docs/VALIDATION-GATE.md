# Gravity Validation Gate

Gravity v1.8.1 is a safety and measurement release before v1.8 Production CLOB.

## Purpose

The validation gate makes sure performance work stays grounded in repeatable data.

It verifies:

- formatting
- debug build
- release build
- optional Cranelift JIT feature build
- workspace tests
- full benchmark execution
- JSON/CSV/Markdown report generation

## Windows

```bat
scripts\release-gate.bat
```

## Linux/macOS

```bash
scripts/release-gate.sh
```

## Benchmark verdicts

The benchmark report now includes current phase targets and verdicts.

| Verdict | Meaning |
|---|---|
| pass | Meets or exceeds the release target |
| watch | At least 50% of target, but still needs tuning |
| fail | Active bottleneck for the following performance pass |

## Current target phases

| Phase | Target ops/sec |
|---|---:|
| insert | 1,000,000 |
| batch-insert | 2,000,000 |
| snapshot | 500,000 |
| cancel | 100,000 |
| match | 1,000,000 |
| compress | 50,000 |
| wire | 5,000,000 |
| wire-batch | 10,000 |
| parallel-markets | 3,000,000 |
| tiles | 10,000,000 |

These are tuning targets, not protocol requirements. The purpose is to expose bottlenecks before the v1.8 CLOB engine pass.
