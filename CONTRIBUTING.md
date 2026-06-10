# Contributing

Thanks for helping improve Gravity.

## Local checks

Before opening a pull request, run:

```bash
./scripts/release-gate.sh
```

On Windows:

```bat
scripts\release-gate.bat
```

## Development rules

- Keep financial math fixed-point and deterministic.
- Do not change CLOB price-time priority without tests and design notes.
- Do not add unbounded queues to hot paths.
- Keep JIT kernels optional, deterministic, and backed by native fallback.
- Add tests for every behavior change in settlement, risk, oracle, AMM, CLOB, WAL, perps, and index logic.
- Do not commit runtime outputs, secrets, `.env` files, benchmarks, or WAL data.

## Pull request checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace --release`
- [ ] Benchmarks still produce reports under `runtime/reports/`
- [ ] Docs updated when public behavior changes
