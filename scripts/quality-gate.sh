#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COVERAGE_FLOOR="${G7_COVERAGE_FLOOR:-75}"

cd "${ROOT_DIR}"

echo "[quality-gate] quick gate"
bash scripts/quick-gate.sh
echo "[quality-gate] cargo test"
cargo test
echo "[quality-gate] cargo clippy"
cargo clippy --all-targets -- -D warnings
echo "[quality-gate] cargo doc"
cargo doc --no-deps
echo "[quality-gate] cargo llvm-cov"
cargo llvm-cov --workspace --all-targets --summary-only --fail-under-lines "${COVERAGE_FLOOR}"

echo "[quality-gate] web build"
(cd web && bun install --frozen-lockfile && (bun run build || npm run build))

if [[ "${G7_WEB_E2E:-0}" == "1" ]]; then
  echo "[quality-gate] web browser e2e"
  (cd web && bunx playwright install chromium && bun run e2e)
fi

echo "[quality-gate] done"
