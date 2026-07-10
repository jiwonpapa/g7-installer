#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${G7_RELEASE_OUT_DIR:-${ROOT_DIR}/dist/release}"
TARGETS=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
)

cd "${ROOT_DIR}"
rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

for target in "${TARGETS[@]}"; do
  cargo build --locked --release --target "${target}" -p g7-cli --bin g7inst
  install -m 0755 "target/${target}/release/g7inst" "${OUT_DIR}/g7inst-${target}"
done

install -m 0644 scripts/bootstrap.sh "${OUT_DIR}/bootstrap.sh"
cargo metadata --locked --format-version 1 >"${OUT_DIR}/cargo-metadata.json"
python3 scripts/generate-sbom.py "${OUT_DIR}/cargo-metadata.json" "${OUT_DIR}/sbom.cdx.json"
(
  cd "${OUT_DIR}"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum g7inst-* bootstrap.sh cargo-metadata.json sbom.cdx.json >checksums.txt
  else
    shasum -a 256 g7inst-* bootstrap.sh cargo-metadata.json sbom.cdx.json >checksums.txt
  fi
)

echo "release assets prepared at ${OUT_DIR}"
