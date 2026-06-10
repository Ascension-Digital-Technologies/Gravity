# Gravity Cranelift JIT Plan

Gravity v1.6 introduces a safe JIT boundary without forcing JIT code onto the critical path.

## Rules

1. JIT is optional and disabled by default.
2. Only deterministic kernels are eligible.
3. JIT output must match the pure Rust reference path.
4. The reference path remains available for rollback.
5. No JIT code may directly mutate settlement-critical state without verification.

## First candidate kernels

- fixed-point risk ratio calculations
- oracle median/VWAP helpers
- settlement compression math
- binary frame validation helpers

## Build flag

```bash
cargo build --features gravity-tile/cranelift-jit
```

This validates the Cranelift host stack while keeping default builds lightweight.
