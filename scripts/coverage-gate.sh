#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COVERAGE_FLOOR="${G7_COVERAGE_FLOOR:-77}"
OWN_TARGET_DIR="${CARGO_TARGET_DIR:-}"
CLEAN_TARGET=0

if [[ -z "${OWN_TARGET_DIR}" ]]; then
  OWN_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/g7inst-coverage-target.XXXXXX")"
  CLEAN_TARGET=1
fi

export CARGO_TARGET_DIR="${OWN_TARGET_DIR}"
TARGET_DIR="${CARGO_TARGET_DIR}"
COVERAGE_JSON="${G7_COVERAGE_JSON:-${TARGET_DIR}/llvm-cov.json}"

cd "${ROOT_DIR}"
mkdir -p "$(dirname "${COVERAGE_JSON}")"

cleanup() {
  if [[ "${CLEAN_TARGET}" == "1" && "${G7_COVERAGE_KEEP_TARGET:-0}" != "1" ]]; then
    rm -rf "${OWN_TARGET_DIR}"
  fi
}
trap cleanup EXIT

echo "[coverage-gate] isolated cargo target: ${CARGO_TARGET_DIR}"
echo "[coverage-gate] cargo llvm-cov"
cargo llvm-cov --locked --workspace --all-targets --json --output-path "${COVERAGE_JSON}" --fail-under-lines "${COVERAGE_FLOOR}"
python3 scripts/check-coverage-ratchet.py "${COVERAGE_JSON}" "${ROOT_DIR}"

echo "[coverage-gate] done"
