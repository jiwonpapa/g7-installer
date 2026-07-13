#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COVERAGE_FLOOR="${G7_COVERAGE_FLOOR:-77}"

cd "${ROOT_DIR}"

echo "[quality-gate] quick gate"
bash scripts/quick-gate.sh
echo "[quality-gate] cargo test"
cargo test --locked --workspace
echo "[quality-gate] cargo clippy"
cargo clippy --locked --workspace --all-targets -- -D warnings
echo "[quality-gate] rustdoc gate"
bash scripts/rustdoc-gate.sh
echo "[quality-gate] cargo audit"
cargo audit
echo "[quality-gate] cargo deny"
cargo deny check
echo "[quality-gate] cargo llvm-cov"
cargo llvm-cov --locked --workspace --all-targets --json --output-path target/llvm-cov.json --fail-under-lines "${COVERAGE_FLOOR}"
python3 scripts/check-coverage-ratchet.py target/llvm-cov.json "${ROOT_DIR}"

echo "[quality-gate] web build"
(cd web && bun install --frozen-lockfile && (bun run build || npm run build))

if [[ "${G7_WEB_E2E:-0}" == "1" ]]; then
  echo "[quality-gate] web browser e2e"
  (cd web && bunx playwright install chromium && bun run e2e)
fi

echo "[quality-gate] done"
