#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${ROOT_DIR}"

reject() {
  local path="$1"
  local pattern="$2"
  local message="$3"

  if rg -q "${pattern}" "${path}"; then
    echo "${message}" >&2
    exit 1
  fi
}

need() {
  local path="$1"
  local pattern="$2"

  if ! rg -q "${pattern}" "${path}"; then
    echo "missing setup auth contract in ${path}: ${pattern}" >&2
    exit 1
  fi
}

need crates/g7-cli/src/web_setup.rs "ensure_setup_runs_as_root"
need crates/g7-cli/src/web_setup.rs "g7inst setup must be started with sudo/root"
need crates/g7-cli/src/web_setup.rs "Server account password input is not used in the web UI"
need crates/g7-cli/src/web_setup.rs "sudo-token"
need web/index.html "서버 비밀번호 입력 없음"
need web/app.js "setup token session is required"

reject crates/g7-cli/src/web_setup.rs "/api/auth/login|LoginRequest|verify_server_account_password|account_can_install|require_login_allowed|require_loopback_login" "setup auth regression: password login backend returned"
reject web/index.html "login-password|login-username|login-form|서버 계정|로그인하고 계속" "setup auth regression: password login UI returned"
reject web/app.js "/api/auth/login|login-password|login-username|login-form" "setup auth regression: password login frontend returned"

if [[ "$(id -u)" == "0" ]]; then
  echo "setup auth smoke skipped runtime non-root check because current user is root"
  exit 0
fi

set +e
output="$(cargo run -q -p g7-cli -- setup --domain example.com 2>&1)"
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
  echo "setup auth regression: setup started without sudo/root" >&2
  exit 1
fi

if [[ "${output}" != *"g7inst setup must be started with sudo/root"* ]]; then
  echo "setup auth regression: missing sudo/root failure message" >&2
  printf '%s\n' "${output}" >&2
  exit 1
fi

if [[ "${output}" != *"Server account password input is not used in the web UI"* ]]; then
  echo "setup auth regression: missing no-password guidance" >&2
  printf '%s\n' "${output}" >&2
  exit 1
fi

echo "setup auth smoke passed"
