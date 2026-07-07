#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${G7_OPS_HOST:-g7-test}"
DOMAIN="${G7_OPS_DOMAIN:-g7-test.local}"
SOURCE="${G7_OPS_SOURCE:-release}"
TARGET="${G7_TARGET:-x86_64-unknown-linux-musl}"
CLI_BIN="${G7_CLI_BIN:-g7inst}"
REPO="${G7_INSTALL_REPO:-jiwonpapa/g7-installer}"
EXPECTED_VERSION="${G7_OPS_EXPECT_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "${ROOT_DIR}/crates/g7-cli/Cargo.toml" | head -n 1)}"
INSTALL_VERSION="${G7_OPS_VERSION:-v${EXPECTED_VERSION}}"
SUDO="${G7_OPS_SUDO:-sudo -n}"
VERIFY_REINSTALL="${G7_OPS_VERIFY_REINSTALL:-1}"
CLEANUP="${G7_OPS_CLEANUP:-1}"
CONFIRM_DISPOSABLE="${G7_OPS_CONFIRM_DISPOSABLE:-0}"
REPORT_DIR="${G7_OPS_REPORT_DIR:-${ROOT_DIR}/target/ops-harness/$(date +%Y%m%d-%H%M%S)}"
BOOTSTRAP_URL="https://raw.githubusercontent.com/${REPO}/main/scripts/bootstrap.sh"

case "${SOURCE}" in
  release)
    REMOTE_BIN="${G7_OPS_REMOTE_BIN:-/usr/local/bin/${CLI_BIN}}"
    ;;
  local)
    REMOTE_BIN="${G7_OPS_REMOTE_BIN:-/tmp/${CLI_BIN}}"
    ;;
  *)
    echo "unsupported G7_OPS_SOURCE: ${SOURCE} (use release or local)" >&2
    exit 2
    ;;
esac

if [[ "${CONFIRM_DISPOSABLE}" != "1" ]]; then
  cat >&2 <<EOF
Refusing to run destructive ops harness.
Set G7_OPS_CONFIRM_DISPOSABLE=1 after confirming ${HOST} is a disposable Ubuntu test VPS.
EOF
  exit 2
fi

need_local() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing local command: $1" >&2
    exit 2
  fi
}

quote() {
  printf "'%s'" "$(printf "%s" "$1" | sed "s/'/'\\\\''/g")"
}

log() {
  printf '[ops-harness] %s\n' "$*"
}

fail() {
  printf '[ops-harness] failed: %s\n' "$*" >&2
  exit 1
}

remote() {
  ssh -n "${HOST}" "$1"
}

capture_remote() {
  local label="$1"
  local command="$2"
  local output

  if ! output="$(remote "${command}" 2>&1)"; then
    printf '%s\n' "${output}" >"${REPORT_DIR}/${label}.failed.log"
    fail "${label} command failed; see ${REPORT_DIR}/${label}.failed.log"
  fi

  printf '%s\n' "${output}" >"${REPORT_DIR}/${label}.log"
  printf '%s\n' "${output}"
}

sudo_capture() {
  local label="$1"
  local command="$2"
  capture_remote "${label}" "${SUDO} ${command}"
}

sudo_sh_capture() {
  local label="$1"
  local command="$2"
  capture_remote "${label}" "${SUDO} sh -c $(quote "${command}")"
}

assert_contains() {
  local label="$1"
  local haystack="$2"
  local needle="$3"

  if [[ "${haystack}" != *"${needle}"* ]]; then
    fail "${label} did not contain expected text: ${needle}"
  fi
}

assert_not_installed() {
  local cycle="$1"
  local package="$2"
  local package_q
  local status

  package_q="$(quote "${package}")"
  status="$(sudo_capture "${cycle}-package-${package}-status" "dpkg-query -W -f='\\\${Status}' ${package_q} 2>/dev/null || true")"
  if [[ "${status}" == "install ok installed" ]]; then
    fail "package still installed after rollback: ${package}"
  fi
}

assert_installer_paths_absent() {
  local cycle="$1"
  sudo_sh_capture "${cycle}-installer-paths-absent" "for path in /etc/g7-installer /var/lib/g7-installer /var/log/g7-installer /var/backups/g7-installer /home/g7/public_html; do test ! -e \"\${path}\" || exit 1; done"
}

validate_report() {
  local report_path="$1"
  python3 - "$report_path" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)

if data.get("phase") != "vhost-enabled":
    raise SystemExit(f"unexpected phase: {data.get('phase')}")

baseline = data.get("preinstall_package_checks") or []
if not baseline:
    raise SystemExit("missing preinstall_package_checks")

for section in ("package_checks", "service_checks", "port_checks", "vhost_checks"):
    checks = data.get(section) or []
    failed = [f"{item.get('name')}: {item.get('message')}" for item in checks if item.get("status") != "pass"]
    if failed:
        raise SystemExit(f"{section} failed: {', '.join(failed)}")

PY
}

write_new_package_list() {
  local report_path="$1"
  local output_path="$2"
  python3 - "$report_path" >"${output_path}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)

for item in data.get("preinstall_package_checks") or []:
    if item.get("status") == "not-installed":
        print(item.get("name"))
PY
}

install_binary() {
  local remote_bin_q
  remote_bin_q="$(quote "${REMOTE_BIN}")"

  case "${SOURCE}" in
    release)
      log "installing release ${INSTALL_VERSION} on ${HOST}"
      capture_remote "bootstrap-download" "curl -fsSL $(quote "${BOOTSTRAP_URL}") -o /tmp/g7-bootstrap.sh"
      sudo_capture "bootstrap-install" "env G7_INSTALL_REPO=$(quote "${REPO}") G7_INSTALL_VERSION=$(quote "${INSTALL_VERSION}") bash /tmp/g7-bootstrap.sh"
      ;;
    local)
      log "building local ${TARGET} binary"
      cargo build --release --target "${TARGET}" -p g7-cli --bin "${CLI_BIN}"
      scp "${ROOT_DIR}/target/${TARGET}/release/${CLI_BIN}" "${HOST}:${REMOTE_BIN}"
      capture_remote "local-binary-chmod" "chmod +x ${remote_bin_q}"
      ;;
  esac
}

cleanup_previous_state() {
  local remote_bin_q
  remote_bin_q="$(quote "${REMOTE_BIN}")"

  log "cleaning previous installer state if present"
  capture_remote "pre-clean" "if test -x ${remote_bin_q}; then ${SUDO} ${remote_bin_q} rollback --yes >/tmp/g7-ops-pre-rollback.log 2>&1 || true; ${SUDO} ${remote_bin_q} reset --yes >/tmp/g7-ops-pre-reset.log 2>&1 || true; fi"
}

run_install_cycle() {
  local cycle="$1"
  local remote_bin_q
  local domain_q
  local doctor_before
  local install_output
  local doctor_after_install
  local report_json
  local rollback_dry_run
  local rollback_output
  local doctor_after_rollback
  local report_path
  local package_list_path

  remote_bin_q="$(quote "${REMOTE_BIN}")"
  domain_q="$(quote "${DOMAIN}")"
  report_path="${REPORT_DIR}/${cycle}-report.json"
  package_list_path="${REPORT_DIR}/${cycle}-new-packages.txt"

  log "${cycle}: preflight doctor"
  doctor_before="$(sudo_capture "${cycle}-doctor-before" "${remote_bin_q} doctor")"
  assert_contains "${cycle} doctor before" "${doctor_before}" "install_allowed: true"

  log "${cycle}: plan"
  capture_remote "${cycle}-plan" "${remote_bin_q} plan --local-test --domain ${domain_q}"

  log "${cycle}: install"
  install_output="$(sudo_capture "${cycle}-install" "${remote_bin_q} install --local-test --domain ${domain_q}")"
  assert_contains "${cycle} install" "${install_output}" "phase: vhost-enabled"

  report_json="$(sudo_capture "${cycle}-report-json" "cat /var/log/g7-installer/report.json")"
  printf '%s\n' "${report_json}" >"${report_path}"
  validate_report "${report_path}"
  write_new_package_list "${report_path}" "${package_list_path}"

  log "${cycle}: post-install doctor must block fresh install"
  doctor_after_install="$(sudo_capture "${cycle}-doctor-after-install" "${remote_bin_q} doctor")"
  assert_contains "${cycle} doctor after install" "${doctor_after_install}" "install_allowed: false"

  log "${cycle}: rollback dry-run"
  rollback_dry_run="$(sudo_capture "${cycle}-rollback-dry-run" "${remote_bin_q} rollback --dry-run")"
  assert_contains "${cycle} rollback dry-run" "${rollback_dry_run}" "G7 Installer Rollback"

  log "${cycle}: rollback"
  rollback_output="$(sudo_capture "${cycle}-rollback" "${remote_bin_q} rollback --yes")"
  assert_contains "${cycle} rollback" "${rollback_output}" "G7 Installer Rollback"

  log "${cycle}: verify removed packages"
  while IFS= read -r package; do
    [[ -z "${package}" ]] && continue
    assert_not_installed "${cycle}" "${package}"
  done <"${package_list_path}"

  log "${cycle}: verify metadata removed"
  sudo_sh_capture "${cycle}-metadata-removed" "test ! -e /var/lib/g7-installer/state.json && test ! -e /var/lib/g7-installer/owned-files.json && test ! -e /var/lib/g7-installer/rollback.json && test ! -e /var/log/g7-installer/report.json && test ! -e /etc/g7-installer/config.toml && test ! -e /etc/g7-installer/local-hosts.txt"
  assert_installer_paths_absent "${cycle}"

  doctor_after_rollback="$(sudo_capture "${cycle}-doctor-after-rollback" "${remote_bin_q} doctor")"
  assert_contains "${cycle} doctor after rollback" "${doctor_after_rollback}" "install_allowed: true"
}

need_local ssh
need_local scp
need_local python3

mkdir -p "${REPORT_DIR}"
log "writing artifacts to ${REPORT_DIR}"

capture_remote "host-baseline" "uname -a; cat /etc/os-release; id"
capture_remote "ubuntu-24-check" ". /etc/os-release && test \"\${ID}\" = ubuntu && test \"\${VERSION_ID}\" = 24.04"

install_binary
cleanup_previous_state

version_output="$(capture_remote "g7-version" "$(quote "${REMOTE_BIN}") --version")"
assert_contains "version" "${version_output}" "${EXPECTED_VERSION}"

run_install_cycle "cycle1"

if [[ "${VERIFY_REINSTALL}" == "1" ]]; then
  run_install_cycle "cycle2"
fi

if [[ "${CLEANUP}" == "0" ]]; then
  log "cleanup disabled; leaving final server state from last cycle"
else
  log "cleanup verified through final rollback"
fi

log "PASS"
