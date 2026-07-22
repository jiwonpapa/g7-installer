#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

echo "[quick-gate] static gate"
bash scripts/static-gate.sh
echo "[quick-gate] setup auth runtime smoke"
G7_SETUP_AUTH_RUNTIME=1 bash scripts/setup-auth-smoke.sh
echo "[quick-gate] cargo fmt"
cargo fmt --check
echo "[quick-gate] state and system adapter unit tests"
cargo test --locked -p g7-state -p g7-system --lib
echo "[quick-gate] g7-core unit tests"
cargo test --locked -p g7-core --lib
echo "[quick-gate] g7-cli web/controller tests"
cargo test --locked -p g7-cli --bin g7inst

echo "[quick-gate] done"
