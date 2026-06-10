#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "[gravity] starting gravityd"
cargo run -p gravityd -- "$@"
