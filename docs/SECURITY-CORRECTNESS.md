# Gravity v3.3.0 Security + Financial Correctness Hardening

Gravity v3.3.0 adds the safety suite needed before deeper production persistence and final performance work.

## Covered invariant groups

- Fixed-point parsing, overflow, division-by-zero, positive price/quantity rules.
- CLOB price-time priority, cancel isolation, market/IOC/FOK non-resting behavior, tick/lot/minimum validation, status fail-closed behavior.
- Settlement idempotency, deterministic fill roots, and base-asset conservation across compressed deltas.
- AMM positive reserve checks, min-output slippage guard, oracle deviation guard, and weighted pool config validation.
- Risk health classification, oracle dependency capture, and liquidation-triggering negative equity cases.
- Perps margin/leverage fail-closed behavior and mark-price PnL determinism.
- Index weight validation, NAV dependency tracking, mint minimum enforcement, and redeem fee paths.
- WAL recovery clean-stream detection and malformed/corruption reporting foundations.
- JIT/native deterministic equivalence for all default safe kernels.

## Guardrails

Gravity must never let performance tuning change financial correctness. The following remain fixed rules:

- CLOB price-time priority is deterministic.
- Settlement idempotency keys must be stable for the same fill set.
- Fixed-point math must fail closed on checked overflow paths.
- AMM swaps cannot violate reserve positivity or explicit slippage guards.
- Risk and liquidation cannot silently proceed without oracle dependencies.
- JIT kernels must always have native fallbacks and exact equivalence checks.

## Validation

Run:

```bat
scripts\release-gate.bat
```

or individually:

```bat
scripts\build.bat
scripts\test.bat
scripts\bench.bat
cargo build --features gravity-tile/cranelift-jit
```
