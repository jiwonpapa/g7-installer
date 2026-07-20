#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

echo "[local-release-gate] quality gate"
bash scripts/quality-gate.sh

echo "[local-release-gate] release assets"
bash scripts/build-release-assets.sh

echo "[local-release-gate] done"
