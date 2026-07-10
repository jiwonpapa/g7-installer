#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

if [[ -n "${RUSTDOCFLAGS:-}" ]]; then
  export RUSTDOCFLAGS="${RUSTDOCFLAGS} -D warnings"
else
  export RUSTDOCFLAGS="-D warnings"
fi

echo "[rustdoc-gate] cargo doc with warnings as errors"
cargo doc --locked --workspace --no-deps --document-private-items
