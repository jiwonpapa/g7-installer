#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"
source scripts/gate-env.sh
g7_prepare_cargo_target "local-release-gate" "${G7_RELEASE_TARGET_DIR:-}"

cleanup() {
  local status=$?
  g7_cleanup_temp_cargo_target "${G7_RELEASE_KEEP_TARGET:-0}"
  if [[ "${status}" == "0" ]]; then
    if [[ "${G7_RELEASE_CLEAN_WEB_DEPS:-0}" == "1" ]]; then
      bash scripts/clean-artifacts.sh --yes --keep-release --quiet
    else
      bash scripts/clean-artifacts.sh --yes --keep-release --keep-web-deps --quiet
    fi
  else
    echo "[local-release-gate] failed; keeping non-target artifacts for debugging" >&2
  fi
  exit "${status}"
}
trap cleanup EXIT

echo "[local-release-gate] quality gate"
bash scripts/quality-gate.sh

echo "[local-release-gate] coverage gate"
bash scripts/coverage-gate.sh

echo "[local-release-gate] release assets"
bash scripts/build-release-assets.sh

echo "[local-release-gate] done"
