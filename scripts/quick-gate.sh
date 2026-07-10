#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

echo "[quick-gate] shell syntax"
bash -n scripts/*.sh
python3 -m py_compile scripts/generate-sbom.py
echo "[quick-gate] web static smoke"
bash scripts/web-static-smoke.sh
echo "[quick-gate] setup auth smoke"
bash scripts/setup-auth-smoke.sh
echo "[quick-gate] javascript syntax"
node --check web/app.js
echo "[quick-gate] cargo fmt"
cargo fmt --check
echo "[quick-gate] g7-core unit tests"
cargo test --locked -p g7-core --lib
echo "[quick-gate] g7-cli web/controller tests"
cargo test --locked -p g7-cli --bin g7inst

echo "[quick-gate] done"
