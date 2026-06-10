## Summary

## Safety checklist

- [ ] Financial math remains fixed-point/deterministic
- [ ] CLOB price-time priority is unchanged or tested
- [ ] Settlement idempotency is unchanged or tested
- [ ] WAL/replay ordering is unchanged or tested
- [ ] JIT changes have native fallback and equivalence tests

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace --release`
- [ ] Benchmarks/report generation checked when performance-sensitive
