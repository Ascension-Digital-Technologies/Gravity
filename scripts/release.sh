#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "[gravity] building release binaries"
cargo build --workspace --release
