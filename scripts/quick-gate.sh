#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

echo "[quick-gate] shell syntax"
bash -n scripts/*.sh
python3 -m py_compile scripts/generate-sbom.py
python3 -m py_compile scripts/ops_harness.py
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p 'test_*.py'
echo "[quick-gate] web static smoke"
bash scripts/web-static-smoke.sh
echo "[quick-gate] setup auth smoke"
bash scripts/setup-auth-smoke.sh
echo "[quick-gate] javascript syntax"
node --check web/app.js
echo "[quick-gate] public documentation scope"
if rg -n -i 'wordpress|워드프레스' README.md SPEC.md DEVELOPMENT_CONSTITUTION.md CHANGELOG.md docs; then
  echo "public documentation must describe the G7-only app scope" >&2
  exit 1
fi
if rg -n -i 'mariadb' README.md SPEC.md DEVELOPMENT_CONSTITUTION.md docs; then
  echo "public documentation must describe the MySQL-only database scope" >&2
  exit 1
fi
if rg -n -i 'wordpress|mariadb' .github/workflows/ops-harness.yml scripts/ops-harness.sh scripts/ops_harness.py; then
  echo "ops harness must match the G7/Laravel and MySQL-only product scope" >&2
  exit 1
fi
echo "[quick-gate] cargo fmt"
cargo fmt --check
echo "[quick-gate] state and system adapter unit tests"
cargo test --locked -p g7-state -p g7-system --lib
echo "[quick-gate] g7-core unit tests"
cargo test --locked -p g7-core --lib
echo "[quick-gate] g7-cli web/controller tests"
cargo test --locked -p g7-cli --bin g7inst

echo "[quick-gate] done"
