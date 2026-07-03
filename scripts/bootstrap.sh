#!/usr/bin/env bash
set -euo pipefail

REPO="${G7_INSTALL_REPO:-jiwonpapa/g7-installer}"
VERSION="${G7_INSTALL_VERSION:-latest}"
INSTALL_DIR="${G7_INSTALL_DIR:-/usr/local/bin}"
BIN_NAME="${G7_INSTALL_BIN:-g7}"

need_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  if [[ "${os}" != "Linux" ]]; then
    echo "unsupported OS: ${os}. G7 Installer currently supports Linux servers only." >&2
    exit 1
  fi

  case "${arch}" in
    x86_64 | amd64)
      echo "x86_64-unknown-linux-musl"
      ;;
    aarch64 | arm64)
      echo "aarch64-unknown-linux-musl"
      ;;
    *)
      echo "unsupported architecture: ${arch}" >&2
      exit 1
      ;;
  esac
}

download_base_url() {
  if [[ "${VERSION}" == "latest" ]]; then
    echo "https://github.com/${REPO}/releases/latest/download"
  else
    echo "https://github.com/${REPO}/releases/download/${VERSION}"
  fi
}

cleanup() {
  if [[ -n "${TMP_DIR:-}" && -d "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}

if [[ "$(id -u)" != "0" ]]; then
  echo "install requires root. Run with: curl -fsSL <bootstrap-url> | sudo bash" >&2
  exit 1
fi

need_command curl
need_command sha256sum
need_command awk
need_command install
need_command uname

TARGET="$(detect_target)"
ASSET="g7-${TARGET}"
BASE_URL="$(download_base_url)"
TMP_DIR="$(mktemp -d)"
trap cleanup EXIT

BIN_PATH="${TMP_DIR}/${ASSET}"
CHECKSUM_PATH="${TMP_DIR}/checksums.txt"

echo "Downloading ${ASSET} from ${REPO} ${VERSION}..."
curl -fsSL "${BASE_URL}/${ASSET}" -o "${BIN_PATH}"
curl -fsSL "${BASE_URL}/checksums.txt" -o "${CHECKSUM_PATH}"

EXPECTED="$(awk -v name="${ASSET}" '$2 == name { print $1 }' "${CHECKSUM_PATH}")"
if [[ -z "${EXPECTED}" ]]; then
  echo "checksum entry not found for ${ASSET}" >&2
  exit 1
fi

ACTUAL="$(sha256sum "${BIN_PATH}" | awk '{ print $1 }')"
if [[ "${EXPECTED}" != "${ACTUAL}" ]]; then
  echo "checksum mismatch for ${ASSET}" >&2
  exit 1
fi

install -d -m 0755 "${INSTALL_DIR}"
install -m 0755 "${BIN_PATH}" "${INSTALL_DIR}/${BIN_NAME}"

echo "Installed ${BIN_NAME} to ${INSTALL_DIR}/${BIN_NAME}"
"${INSTALL_DIR}/${BIN_NAME}" --version
echo
echo "Next:"
echo "  ${BIN_NAME} doctor"
echo "  sudo ${BIN_NAME} install --domain example.com"
