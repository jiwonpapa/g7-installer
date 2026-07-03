#!/usr/bin/env bash
set -euo pipefail

VM_HOST="${G7_VM_HOST:-g7-test}"
RESET_HOST="${G7_RESET_HOST:-localubuntu}"
RESET_COMMAND="${G7_RESET_COMMAND:-sudo -n /usr/local/bin/g7-vm-reset}"
DOMAIN="${G7_SMOKE_DOMAIN:-g7-test.local}"
TARGET="${G7_TARGET:-x86_64-unknown-linux-musl}"
CLI_BIN="${G7_CLI_BIN:-g7inst}"
BIN="target/${TARGET}/release/${CLI_BIN}"
REMOTE_BIN="/tmp/${CLI_BIN}"

cargo build --release --target "${TARGET}" -p g7-cli

scp "${BIN}" "${VM_HOST}:${REMOTE_BIN}"
ssh "${VM_HOST}" "chmod +x '${REMOTE_BIN}'"

ssh "${VM_HOST}" "sudo -n '${REMOTE_BIN}' doctor"
ssh "${VM_HOST}" "'${REMOTE_BIN}' plan --local-test --domain '${DOMAIN}'"
ssh "${VM_HOST}" "sudo -n '${REMOTE_BIN}' install --local-test --domain '${DOMAIN}'"

if ssh "${VM_HOST}" "sudo -n '${REMOTE_BIN}' doctor"; then
  echo "post-install doctor unexpectedly allowed a fresh install" >&2
  exit 1
fi

ssh "${VM_HOST}" "sudo -n '${REMOTE_BIN}' reset --yes"
ssh "${VM_HOST}" "sudo -n '${REMOTE_BIN}' doctor"

if [[ "${G7_SMOKE_RESET:-0}" == "1" ]]; then
  ssh "${RESET_HOST}" "${RESET_COMMAND}"
  scp "${BIN}" "${VM_HOST}:${REMOTE_BIN}"
  ssh "${VM_HOST}" "chmod +x '${REMOTE_BIN}' && sudo -n '${REMOTE_BIN}' doctor"
fi
