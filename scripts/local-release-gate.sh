#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWN_TARGET_DIR="${G7_RELEASE_TARGET_DIR:-}"
CLEAN_TARGET=0

cd "${ROOT_DIR}"

if [[ -z "${OWN_TARGET_DIR}" ]]; then
  OWN_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/g7inst-release-target.XXXXXX")"
  CLEAN_TARGET=1
fi

cleanup() {
  if [[ "${CLEAN_TARGET}" == "1" && "${G7_RELEASE_KEEP_TARGET:-0}" != "1" ]]; then
    rm -rf "${OWN_TARGET_DIR}"
  fi
}
trap cleanup EXIT

export CARGO_TARGET_DIR="${OWN_TARGET_DIR}"
echo "[local-release-gate] isolated cargo target: ${CARGO_TARGET_DIR}"

echo "[local-release-gate] quality gate"
bash scripts/quality-gate.sh

echo "[local-release-gate] coverage gate"
bash scripts/coverage-gate.sh

echo "[local-release-gate] release assets"
bash scripts/build-release-assets.sh

echo "[local-release-gate] done"
