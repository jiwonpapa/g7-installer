#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COVERAGE_FLOOR="${G7_COVERAGE_FLOOR:-77}"

cd "${ROOT_DIR}"
source scripts/gate-env.sh

g7_prepare_cargo_target "coverage-gate" "${G7_COVERAGE_TARGET_DIR:-}"
TARGET_DIR="${CARGO_TARGET_DIR}"
COVERAGE_JSON="${G7_COVERAGE_JSON:-${TARGET_DIR}/llvm-cov.json}"
mkdir -p "$(dirname "${COVERAGE_JSON}")"

cleanup() {
  local status=$?
  g7_cleanup_temp_cargo_target "${G7_COVERAGE_KEEP_TARGET:-0}"
  if [[ "${status}" == "0" ]]; then
    bash scripts/clean-artifacts.sh --yes --keep-release --keep-web-deps --quiet
  else
    echo "[coverage-gate] failed; keeping non-target artifacts for debugging" >&2
  fi
  exit "${status}"
}
trap cleanup EXIT

echo "[coverage-gate] cargo llvm-cov"
cargo llvm-cov --locked --workspace --all-targets --json --output-path "${COVERAGE_JSON}" --fail-under-lines "${COVERAGE_FLOOR}"
python3 scripts/check-coverage-ratchet.py "${COVERAGE_JSON}" "${ROOT_DIR}"

echo "[coverage-gate] done"
