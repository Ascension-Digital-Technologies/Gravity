#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "[gravity] building workspace"
cargo build --workspace
