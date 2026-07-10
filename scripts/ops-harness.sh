#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${G7_OPS_HOST:-g7-test}"
DOMAIN="${G7_OPS_DOMAIN:-}"
SOURCE="${G7_OPS_SOURCE:-release}"
TARGET="${G7_TARGET:-x86_64-unknown-linux-musl}"
CLI_BIN="${G7_CLI_BIN:-g7inst}"
REPO="${G7_INSTALL_REPO:-jiwonpapa/g7-installer}"
EXPECTED_VERSION="${G7_OPS_EXPECT_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "${ROOT_DIR}/crates/g7-cli/Cargo.toml" | head -n 1)}"
INSTALL_VERSION="${G7_OPS_VERSION:-v${EXPECTED_VERSION}}"
SUDO="${G7_OPS_SUDO:-sudo -n}"
VERIFY_REINSTALL="${G7_OPS_VERIFY_REINSTALL:-0}"
CLEANUP="${G7_OPS_CLEANUP:-1}"
CERTBOT_SCOPE="${G7_OPS_CERTBOT_SCOPE:-staging}"
APP_SMOKE="${G7_OPS_APP_SMOKE:-1}"
APP_PROFILE="${G7_OPS_APP:-gnuboard7}"
WEB_SERVER="${G7_OPS_WEB_SERVER:-nginx}"
PHP_VERSION="${G7_OPS_PHP_VERSION:-8.5}"
PHP_SOURCE="${G7_OPS_PHP_SOURCE:-auto}"
DATABASE="${G7_OPS_DATABASE:-mysql}"
REDIS="${G7_OPS_REDIS:-enable}"
MAIL_MODE="${G7_OPS_MAIL_MODE:-none}"
WWW_MODE="${G7_OPS_WWW_MODE:-redirect-to-root}"
STEPS="${G7_OPS_STEPS:-fresh-doctor,plan,install,report-contract,setup-guide,app-smoke,post-install-doctor,reset-dry-run,reset,fresh-doctor-after-reset}"
STEPS="${STEPS// /}"
PRE_CLEAN="${G7_OPS_PRE_CLEAN:-auto}"
ALLOW_LOCAL_TEST="${G7_OPS_ALLOW_LOCAL_TEST:-0}"
CONFIRM_DISPOSABLE="${G7_OPS_CONFIRM_DISPOSABLE:-0}"
REPORT_DIR="${G7_OPS_REPORT_DIR:-${ROOT_DIR}/target/ops-harness/$(date +%Y%m%d-%H%M%S)}"
BOOTSTRAP_URL="https://github.com/${REPO}/releases/download/${INSTALL_VERSION}/bootstrap.sh"

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

case "${CERTBOT_SCOPE}" in
  skip|staging)
    ;;
  production)
    if [[ "${G7_OPS_ALLOW_PRODUCTION_LE:-0}" != "1" ]]; then
      echo "G7_OPS_CERTBOT_SCOPE=production requires G7_OPS_ALLOW_PRODUCTION_LE=1" >&2
      exit 2
    fi
    ;;
  *)
    echo "unsupported G7_OPS_CERTBOT_SCOPE: ${CERTBOT_SCOPE} (use skip, staging, or production)" >&2
    exit 2
    ;;
esac

if [[ -z "${DOMAIN}" ]]; then
  echo "G7_OPS_DOMAIN is required. Use a real DNS domain for the ops harness." >&2
  exit 2
fi

if [[ "${CERTBOT_SCOPE}" == "skip" && "${ALLOW_LOCAL_TEST}" != "1" ]]; then
  cat >&2 <<EOF
G7_OPS_CERTBOT_SCOPE=skip runs --local-test and is disabled by default.
Use G7_OPS_CERTBOT_SCOPE=staging with a real DNS domain, or set G7_OPS_ALLOW_LOCAL_TEST=1 only for an explicit legacy local-test harness.
EOF
  exit 2
fi

if [[ "${DOMAIN}" == *.local && "${ALLOW_LOCAL_TEST}" != "1" ]]; then
  echo "local-test domain ${DOMAIN} requires G7_OPS_ALLOW_LOCAL_TEST=1." >&2
  exit 2
fi

if [[ "${CERTBOT_SCOPE}" != "skip" && "${DOMAIN}" == *.local ]]; then
  echo "G7_OPS_CERTBOT_SCOPE=${CERTBOT_SCOPE} requires a real DNS domain, not ${DOMAIN}" >&2
  exit 2
fi

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

step_enabled() {
  local step="$1"
  [[ ",${STEPS}," == *",all,"* || ",${STEPS}," == *",${step},"* ]]
}

pre_clean_enabled() {
  case "${PRE_CLEAN}" in
    1|true|yes)
      return 0
      ;;
    0|false|no)
      return 1
      ;;
    auto)
      step_enabled install
      return
      ;;
    *)
      echo "unsupported G7_OPS_PRE_CLEAN: ${PRE_CLEAN} (use auto, 1, or 0)" >&2
      exit 2
      ;;
  esac
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

assert_installer_resources_absent() {
  local cycle="$1"
  local report_path="$2"
  local site_user
  local web_root
  local services

  read -r site_user web_root < <(python3 - "${report_path}" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)
print(data["site_user"], data["web_root"])
PY
)
  services="$(python3 - "${report_path}" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)
services = {"redis-server"}
services.add("apache2" if data.get("web_server") == "apache" else "nginx")
services.add("mariadb" if data.get("database") == "mariadb" else "mysql")
if data.get("web_server") == "frankenphp":
    services.add("g7-frankenphp")
for service in sorted(services):
    print(service)
PY
)"

  sudo_sh_capture "${cycle}-installer-paths-absent" "for path in /etc/g7-installer /var/lib/g7-installer /var/log/g7-installer /var/backups/g7-installer $(quote "${web_root}"); do test ! -e \"\${path}\" || exit 1; done"
  sudo_sh_capture "${cycle}-site-account-absent" "! id -u $(quote "${site_user}") >/dev/null 2>&1"
  while IFS= read -r service; do
    [[ -n "${service}" ]] || continue
    sudo_sh_capture "${cycle}-service-${service}-inactive" "! systemctl is-active --quiet $(quote "${service}")"
  done <<<"${services}"
}

validate_report() {
  local report_path="$1"
  python3 - "$report_path" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)

if data.get("schema_version") != 1:
    raise SystemExit(f"unsupported schema_version: {data.get('schema_version')}")

required = (
    "domain", "deployment_mode", "app_profile", "web_server", "php_version",
    "database", "database_name", "database_user", "site_user", "web_root",
)
missing = [key for key in required if not data.get(key)]
if missing:
    raise SystemExit(f"missing required report fields: {', '.join(missing)}")

if data.get("phase") != "completed":
    raise SystemExit(f"unexpected phase: {data.get('phase')}")

baseline = data.get("preinstall_package_checks") or []
if not baseline:
    raise SystemExit("missing preinstall_package_checks")

sections = (
    "safety_checks", "preinstall_package_checks", "package_checks", "service_checks",
    "port_checks", "network_checks", "runtime_checks", "database_checks",
    "firewall_checks", "mail_checks", "certbot_checks", "vhost_checks", "app_checks",
)
for section in sections:
    checks = data.get(section)
    if not isinstance(checks, list):
        raise SystemExit(f"{section} is missing or is not a list")
    failed = [f"{item.get('name')}: {item.get('message')}" for item in checks if item.get("status") == "fail"]
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

install_args() {
  local common
  common="--app $(quote "${APP_PROFILE}") --web-server $(quote "${WEB_SERVER}") --php-version $(quote "${PHP_VERSION}") --php-source $(quote "${PHP_SOURCE}") --database $(quote "${DATABASE}") --redis $(quote "${REDIS}") --mail-mode $(quote "${MAIL_MODE}") --www-mode $(quote "${WWW_MODE}")"
  case "${CERTBOT_SCOPE}" in
    skip)
      printf -- "--local-test --domain %s %s" "$(quote "${DOMAIN}")" "${common}"
      ;;
    staging|production)
      printf -- "--domain %s %s" "$(quote "${DOMAIN}")" "${common}"
      ;;
  esac
}

install_env_prefix() {
  case "${CERTBOT_SCOPE}" in
    staging)
      printf "env G7_CERTBOT_STAGING=1 "
      ;;
    *)
      printf ""
      ;;
  esac
}

run_app_smoke() {
  local cycle="$1"
  local report_path="$2"
  local url
  local deployment_mode
  local smoke_host
  local smoke_port

  if [[ "${APP_SMOKE}" != "1" ]]; then
    log "${cycle}: app smoke skipped (set G7_OPS_APP_SMOKE=1 to enable)"
    return
  fi

  url="$(python3 - "$report_path" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)
print(data.get("app_url") or "")
PY
)"
  if [[ -z "${url}" ]]; then
    fail "${cycle}: report did not contain app_url for app smoke"
  fi

  deployment_mode="$(python3 - "$report_path" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)
print(data.get("deployment_mode") or "")
PY
)"
  if [[ "${deployment_mode}" == "local-test" ]]; then
    read -r smoke_host smoke_port < <(python3 - "$url" <<'PY'
from urllib.parse import urlparse
import sys
parsed = urlparse(sys.argv[1])
port = parsed.port or (443 if parsed.scheme == "https" else 80)
print(parsed.hostname or "", port)
PY
)
    if [[ -z "${smoke_host}" || -z "${smoke_port}" ]]; then
      fail "${cycle}: could not parse app_url for local-test smoke: ${url}"
    fi
    capture_remote "${cycle}-app-smoke" "curl -fsSL --max-time 15 --resolve $(quote "${smoke_host}:${smoke_port}:127.0.0.1") $(quote "${url}") >/dev/null"
  else
    if [[ "${CERTBOT_SCOPE}" == "staging" ]]; then
      capture_remote "${cycle}-app-smoke" "curl -kfsSL --max-time 15 $(quote "${url}") >/dev/null"
    else
      capture_remote "${cycle}-app-smoke" "curl -fsSL --max-time 15 $(quote "${url}") >/dev/null"
    fi
  fi
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
  capture_remote "pre-clean" "if test -x ${remote_bin_q}; then ${SUDO} ${remote_bin_q} rollback --yes >/tmp/g7-ops-pre-rollback.log 2>&1; rollback_status=\$?; ${SUDO} ${remote_bin_q} reset --yes >/tmp/g7-ops-pre-reset.log 2>&1; reset_status=\$?; printf 'rollback_status=%s reset_status=%s\\n' \"\${rollback_status}\" \"\${reset_status}\"; cat /tmp/g7-ops-pre-rollback.log /tmp/g7-ops-pre-reset.log; fi; true"
}

run_install_cycle() {
  local cycle="$1"
  local remote_bin_q
  local doctor_before
  local install_output
  local doctor_after_install
  local doctor_after_reset
  local report_json
  local reset_dry_run_output
  local report_path
  local package_list_path
  local certificate_present="no"
  local site_user
  local args
  local env_prefix

  remote_bin_q="$(quote "${REMOTE_BIN}")"
  report_path="${REPORT_DIR}/${cycle}-report.json"
  package_list_path="${REPORT_DIR}/${cycle}-new-packages.txt"
  args="$(install_args)"
  env_prefix="$(install_env_prefix)"

  if step_enabled fresh-doctor; then
    log "${cycle}: preflight doctor"
    doctor_before="$(sudo_capture "${cycle}-doctor-before" "${remote_bin_q} doctor")"
    assert_contains "${cycle} doctor before" "${doctor_before}" "install_allowed: true"
  else
    log "${cycle}: preflight doctor skipped"
  fi

  if step_enabled plan; then
    log "${cycle}: plan"
    capture_remote "${cycle}-plan" "${remote_bin_q} plan ${args}"
  else
    log "${cycle}: plan skipped"
  fi

  if step_enabled install; then
    log "${cycle}: install"
    install_output="$(sudo_capture "${cycle}-install" "${env_prefix}${remote_bin_q} install ${args}")"
    assert_contains "${cycle} install" "${install_output}" "phase: completed"
  else
    log "${cycle}: install skipped"
  fi

  if step_enabled report-contract || step_enabled setup-guide || step_enabled app-smoke || step_enabled reset; then
    report_json="$(sudo_capture "${cycle}-report-json" "cat /var/log/g7-installer/report.json")"
    printf '%s\n' "${report_json}" >"${report_path}"
    write_new_package_list "${report_path}" "${package_list_path}"
    site_user="$(python3 - "${report_path}" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    print(json.load(handle).get("site_user") or "")
PY
)"
    certificate_present="$(sudo_capture "${cycle}-certificate-before-reset" "if test -d $(quote "/etc/letsencrypt/live/${DOMAIN}"); then echo yes; else echo no; fi")"
  fi

  if step_enabled report-contract; then
    log "${cycle}: install report contract"
    validate_report "${report_path}"
  else
    log "${cycle}: install report contract skipped"
  fi

  if step_enabled setup-guide; then
    log "${cycle}: setup guide capture"
    sudo_capture "${cycle}-setup-guide" "cat /var/log/g7-installer/setup-guide.md" >/dev/null
  else
    log "${cycle}: setup guide capture skipped"
  fi

  if step_enabled app-smoke; then
    run_app_smoke "${cycle}" "${report_path}"
  else
    log "${cycle}: app smoke step skipped"
  fi

  if step_enabled post-install-doctor; then
    log "${cycle}: post-install doctor must block fresh install"
    doctor_after_install="$(sudo_capture "${cycle}-doctor-after-install" "${remote_bin_q} doctor")"
    assert_contains "${cycle} doctor after install" "${doctor_after_install}" "install_allowed: false"
  else
    log "${cycle}: post-install doctor skipped"
  fi

  if [[ "${CLEANUP}" == "1" ]]; then
    if step_enabled reset-dry-run; then
      log "${cycle}: reset dry-run preview"
      reset_dry_run_output="$(sudo_capture "${cycle}-reset-dry-run" "${remote_bin_q} reset --yes --dry-run")"
      assert_contains "${cycle} reset dry-run" "${reset_dry_run_output}" "dry_run: true"
    else
      log "${cycle}: reset dry-run skipped"
    fi

    if step_enabled reset; then
      log "${cycle}: reset installer-created resources"
      reset_output="$(sudo_capture "${cycle}-reset" "${remote_bin_q} reset --yes")"
      assert_contains "${cycle} reset" "${reset_output}" "G7 Installer Reset"
      assert_contains "${cycle} reset actions" "${reset_output}" "actions:"
      assert_contains "${cycle} database reset" "${reset_output}" " database -"
      assert_contains "${cycle} account reset" "${reset_output}" "account:${site_user}"
      while IFS= read -r package; do
        [[ -n "${package}" ]] || continue
        assert_not_installed "${cycle}" "${package}"
      done <"${package_list_path}"
      assert_installer_resources_absent "${cycle}" "${report_path}"
      if [[ "${certificate_present}" == "yes" ]]; then
        sudo_capture "${cycle}-certificate-preserved" "test -d $(quote "/etc/letsencrypt/live/${DOMAIN}")"
      fi
    else
      log "${cycle}: reset skipped"
    fi

    if step_enabled fresh-doctor-after-reset; then
      log "${cycle}: doctor after reset must allow fresh install"
      doctor_after_reset="$(sudo_capture "${cycle}-doctor-after-reset" "${remote_bin_q} doctor")"
      assert_contains "${cycle} doctor after reset" "${doctor_after_reset}" "install_allowed: true"
    else
      log "${cycle}: doctor after reset skipped"
    fi
  else
    log "${cycle}: cleanup disabled; reset steps skipped"
  fi
}

need_local ssh
need_local scp
need_local python3

mkdir -p "${REPORT_DIR}"
log "writing artifacts to ${REPORT_DIR}"
log "steps: ${STEPS}"
log "profile: ${APP_PROFILE}; web: ${WEB_SERVER}; PHP: ${PHP_VERSION}/${PHP_SOURCE}; DB: ${DATABASE}"
log "certbot scope: ${CERTBOT_SCOPE}; app smoke: ${APP_SMOKE}; pre-clean: ${PRE_CLEAN}"

capture_remote "host-baseline" "uname -a; cat /etc/os-release; id"
capture_remote "ubuntu-24-check" ". /etc/os-release && test \"\${ID}\" = ubuntu && test \"\${VERSION_ID}\" = 24.04"

install_binary
if pre_clean_enabled; then
  cleanup_previous_state
else
  log "pre-clean skipped"
fi

version_output="$(capture_remote "g7-version" "$(quote "${REMOTE_BIN}") --version")"
assert_contains "version" "${version_output}" "${EXPECTED_VERSION}"

run_install_cycle "cycle1"

if [[ "${VERIFY_REINSTALL}" == "1" ]]; then
  log "VERIFY_REINSTALL=1 requires the VPS to be restored to a fresh snapshot before cycle2"
  run_install_cycle "cycle2"
fi

if [[ "${CLEANUP}" == "0" ]]; then
  log "cleanup disabled; leaving final server state from last cycle"
else
  log "cleanup reset installer-created resources completed"
fi

log "PASS"
