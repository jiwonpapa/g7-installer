#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VM_HOST="${G7_VM_HOST:-g7-test}"
RESET_HOST="${G7_RESET_HOST:-localubuntu}"
RESET_COMMAND="${G7_RESET_COMMAND:-sudo -n /usr/local/bin/g7-vm-reset}"

if [[ "${G7_SMOKE_RESET:-0}" == "1" ]]; then
  ssh "${RESET_HOST}" "${RESET_COMMAND}"
fi

G7_OPS_HOST="${VM_HOST}" \
G7_OPS_DOMAIN="${G7_SMOKE_DOMAIN:-g7-test.local}" \
G7_OPS_SOURCE="${G7_OPS_SOURCE:-local}" \
G7_OPS_CONFIRM_DISPOSABLE="${G7_OPS_CONFIRM_DISPOSABLE:-1}" \
G7_OPS_VERIFY_REINSTALL="${G7_OPS_VERIFY_REINSTALL:-1}" \
G7_TARGET="${G7_TARGET:-x86_64-unknown-linux-musl}" \
G7_CLI_BIN="${G7_CLI_BIN:-g7inst}" \
"${ROOT_DIR}/scripts/ops-harness.sh"
