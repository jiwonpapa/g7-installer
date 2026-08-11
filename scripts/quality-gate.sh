#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"
source scripts/gate-env.sh

g7_prepare_cargo_target "quality-gate" "${G7_QUALITY_TARGET_DIR:-}"

cleanup() {
  local status=$?
  g7_cleanup_temp_cargo_target "${G7_QUALITY_KEEP_TARGET:-0}"
  if [[ "${status}" == "0" ]]; then
    if [[ "${G7_QUALITY_CLEAN_WEB_DEPS:-0}" == "1" ]]; then
      bash scripts/clean-artifacts.sh --yes --keep-release --quiet
    else
      bash scripts/clean-artifacts.sh --yes --keep-release --keep-web-deps --quiet
    fi
  else
    echo "[quality-gate] failed; keeping non-target artifacts for debugging" >&2
  fi
  exit "${status}"
}
trap cleanup EXIT

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

echo "[quality-gate] web build"
(cd web && bun install --frozen-lockfile && (bun run build || npm run build))

if [[ "${G7_WEB_E2E:-0}" == "1" ]]; then
  echo "[quality-gate] web browser e2e"
  (cd web && bunx playwright install chromium && bun run e2e)
fi

echo "[quality-gate] done"
