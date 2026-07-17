## What changed

Describe the behavior or implementation change and why it is needed.

## Validation

List the commands you ran and any relevant test or benchmark results.

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace --release`

## Safety checks

- [ ] Financial math remains fixed-point and deterministic
- [ ] CLOB price-time priority is unchanged or covered by tests
- [ ] Settlement idempotency is unchanged or covered by tests
- [ ] WAL and replay ordering are unchanged or covered by tests
- [ ] JIT changes retain a native fallback and equivalence coverage

## Notes

Call out public API changes, migration concerns, generated files, or follow-up work.
