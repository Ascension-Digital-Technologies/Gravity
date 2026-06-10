# Gravity v3.0.0 Safe Cranelift JIT Kernels

Gravity v3.0.0 turns the earlier JIT scaffold into a safe deterministic kernel layer.

## Design rules

- Native Rust fallback always remains available.
- JIT/native outputs must match exactly before accelerated output is trusted.
- JIT is not installed into CLOB price-time ordering.
- JIT cannot change replay order, settlement correctness, or fixed-point rules.
- Kernel promotion is opt-in through configuration and benchmark validation.

## Kernel families

- `FeeBps` — maker/taker fee math.
- `MarginRequirement` — initial/maintenance margin calculations.
- `HealthBps` — account health basis-point calculation.
- `NetDelta` — settlement netting helper.
- `AmmQuote` — constant-product quote helper.
- `IndexNav` — weighted NAV helper.

## Build modes

Default build keeps Cranelift disabled:

```bat
cargo build
```

Feature build validates host support:

```bat
cargo build --features gravity-tile/cranelift-jit
```

## Benchmark

The benchmark includes a `jit-kernels` phase:

```bat
scriptsench.bat
```

This phase checks native/accelerated equivalence on every operation.
