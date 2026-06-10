#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "[gravity] running tests"
cargo test --workspace
