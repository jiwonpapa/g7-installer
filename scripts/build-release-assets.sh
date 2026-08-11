#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${G7_RELEASE_OUT_DIR:-${ROOT_DIR}/dist/release}"
TARGETS=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
)

cd "${ROOT_DIR}"
source scripts/gate-env.sh
g7_prepare_cargo_target "build-release-assets" "${G7_RELEASE_TARGET_DIR:-}"
TARGET_DIR="${CARGO_TARGET_DIR}"

cleanup() {
  local status=$?
  g7_cleanup_temp_cargo_target "${G7_RELEASE_KEEP_TARGET:-0}"
  exit "${status}"
}
trap cleanup EXIT

rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/g7-cli/Cargo.toml | head -n 1)"
TAG="v${VERSION}"

python3 scripts/check-release-policy.py

for target in "${TARGETS[@]}"; do
  cargo build --locked --release --target "${target}" -p g7-cli --bin g7inst
  install -m 0755 "${TARGET_DIR}/${target}/release/g7inst" "${OUT_DIR}/g7inst-${target}"
done

awk -v tag="${TAG}" '
  $0 == "VERSION=\"${G7_INSTALL_VERSION:-latest}\"" {
    print "VERSION=\"${G7_INSTALL_VERSION:-" tag "}\""
    next
  }
  { print }
' scripts/bootstrap.sh >"${OUT_DIR}/bootstrap.sh"
chmod 0644 "${OUT_DIR}/bootstrap.sh"
grep -Fq "VERSION=\"\${G7_INSTALL_VERSION:-${TAG}}\"" "${OUT_DIR}/bootstrap.sh"
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
