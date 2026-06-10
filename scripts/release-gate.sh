#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p runtime/reports

echo "[gravity] release gate $(cat VERSION)"

echo "[1/8] cargo fmt check"
cargo fmt --all -- --check

echo "[2/8] cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "[3/8] build workspace debug"
cargo build --workspace

echo "[4/8] build workspace release"
cargo build --workspace --release

echo "[5/8] optional JIT feature build"
cargo build --features gravity-tile/cranelift-jit

echo "[6/8] tests"
cargo test --workspace

echo "[7/8] benchmark"
./scripts/bench.sh

echo "[8/8] report checks"
test -f runtime/reports/gravity-bench.json
test -f runtime/reports/gravity-bench.csv
test -f runtime/reports/gravity-release-report.md

echo "[gravity] release gate passed"
