#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VM_HOST="${G7_VM_HOST:-g7-test}"
RESET_HOST="${G7_RESET_HOST:-localubuntu}"
RESET_COMMAND="${G7_RESET_COMMAND:-sudo -n /usr/local/bin/g7-vm-reset}"

if [[ "${G7_SMOKE_ALLOW_LOCAL_TEST:-0}" != "1" ]]; then
  cat >&2 <<EOF
g7-test local VM smoke is disabled by default.
Use scripts/ops-harness.sh with G7_OPS_DOMAIN=<real-domain> and G7_OPS_CERTBOT_SCOPE=staging for current server validation.
Set G7_SMOKE_ALLOW_LOCAL_TEST=1 only when intentionally running the legacy local-test VM smoke.
EOF
  exit 2
fi

if [[ "${G7_SMOKE_RESET:-0}" == "1" ]]; then
  ssh "${RESET_HOST}" "${RESET_COMMAND}"
fi

G7_OPS_HOST="${VM_HOST}" \
G7_OPS_DOMAIN="${G7_SMOKE_DOMAIN:-g7-test.local}" \
G7_OPS_SOURCE="${G7_OPS_SOURCE:-local}" \
G7_OPS_CERTBOT_SCOPE="${G7_OPS_CERTBOT_SCOPE:-skip}" \
G7_OPS_ALLOW_LOCAL_TEST=1 \
G7_OPS_CONFIRM_DISPOSABLE="${G7_OPS_CONFIRM_DISPOSABLE:-1}" \
G7_OPS_VERIFY_REINSTALL="${G7_OPS_VERIFY_REINSTALL:-1}" \
G7_TARGET="${G7_TARGET:-x86_64-unknown-linux-musl}" \
G7_CLI_BIN="${G7_CLI_BIN:-g7inst}" \
"${ROOT_DIR}/scripts/ops-harness.sh"
