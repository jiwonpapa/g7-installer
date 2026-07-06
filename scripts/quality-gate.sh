#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COVERAGE_FLOOR="${G7_COVERAGE_FLOOR:-75}"

cd "${ROOT_DIR}"

bash -n scripts/*.sh
scripts/web-static-smoke.sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
cargo llvm-cov --workspace --all-targets --summary-only --fail-under-lines "${COVERAGE_FLOOR}"

(cd web && bun install --frozen-lockfile && bun run build)
