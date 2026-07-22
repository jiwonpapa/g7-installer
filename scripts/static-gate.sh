#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

cleanup() {
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
}
trap cleanup EXIT

echo "[static-gate] shell syntax"
bash -n scripts/*.sh
echo "[static-gate] python syntax and harness tests"
python3 -m py_compile \
  scripts/check-coverage-ratchet.py \
  scripts/generate-sbom.py \
  scripts/ops_harness.py \
  scripts/structure-audit.py
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p 'test_*.py'
echo "[static-gate] structure audit"
python3 scripts/structure-audit.py
echo "[static-gate] web static smoke"
bash scripts/web-static-smoke.sh
echo "[static-gate] setup auth static smoke"
G7_SETUP_AUTH_RUNTIME=0 bash scripts/setup-auth-smoke.sh
echo "[static-gate] javascript syntax"
node --check web/app.js
echo "[static-gate] public documentation scope"
if rg -n -i 'wordpress|워드프레스' README.md SPEC.md DEVELOPMENT_CONSTITUTION.md CHANGELOG.md docs; then
  echo "public documentation must describe the G7-only app scope" >&2
  exit 1
fi
if rg -n -i 'mariadb' README.md SPEC.md DEVELOPMENT_CONSTITUTION.md docs; then
  echo "public documentation must describe the MySQL-only database scope" >&2
  exit 1
fi
if rg -n -i 'wordpress|mariadb' scripts/ops-harness.sh scripts/ops_harness.py; then
  echo "ops harness must match the G7/Laravel and MySQL-only product scope" >&2
  exit 1
fi

echo "[static-gate] done"
