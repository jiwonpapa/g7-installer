#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

need_pattern() {
  local file="$1"
  local pattern="$2"

  if ! rg -q "${pattern}" "${ROOT_DIR}/${file}"; then
    echo "missing web UI contract in ${file}: ${pattern}" >&2
    exit 1
  fi
}

reject_pattern() {
  local file="$1"
  local pattern="$2"
  local message="$3"

  if rg -q "${pattern}" "${ROOT_DIR}/${file}"; then
    echo "${message}" >&2
    exit 1
  fi
}

need_pattern "web/index.html" "G7 설치 마법사"
need_pattern "web/index.html" "data-view=\"login\""
need_pattern "web/index.html" "접속 확인"
need_pattern "web/index.html" "서버 비밀번호 입력 없음"
need_pattern "web/index.html" "data-view=\"check\""
need_pattern "web/index.html" "data-view=\"options\""
need_pattern "web/index.html" "data-view=\"plan\""
need_pattern "web/index.html" "data-view=\"install\""
need_pattern "web/index.html" "data-view=\"report\""
need_pattern "web/index.html" "id=\"theme-toggle\""
need_pattern "web/index.html" "class=\"btn btn-sm btn-outline icon-button\""
need_pattern "web/index.html" "data-icon=\"package-plus\""
need_pattern "web/index.html" "data-ui-icon=\"shield-check\""
need_pattern "web/index.html" "id=\"install-progress\""
need_pattern "web/index.html" "id=\"package-progress-list\""
need_pattern "web/index.html" "id=\"install-result-button\""
need_pattern "web/index.html" "id=\"report-progress\""
need_pattern "web/index.html" "id=\"check-next-button\""
need_pattern "web/index.html" "name=\"app_package\""
need_pattern "web/index.html" "name=\"database_version\""
need_pattern "web/index.html" "__G7INST_ASSET_VERSION__"
need_pattern "web/index.html" "id=\"reset-button\""
need_pattern "web/index.html" "id=\"rollback-button\""
need_pattern "web/index.html" "id=\"summary-panel\" class=\"summary-panel\" hidden"
need_pattern "web/index.html" "data-recovery-panel"
need_pattern "web/index.html" "data-recovery-action=\"rollback\""
need_pattern "web/index.html" "id=\"install-confirm-dialog\""
need_pattern "web/index.html" "id=\"recovery-confirm-dialog\""
need_pattern "web/index.html" "id=\"recovery-confirm-yes\""
need_pattern "web/index.html" "id=\"operation-overlay\""
need_pattern "web/index.html" "초기화 중입니다."
need_pattern "web/index.html" "id=\"floating-help\""
need_pattern "web/index.html" "class=\"app-workspace\""
need_pattern "web/index.html" "class=\"workspace-grid\""
need_pattern "web/index.html" "class=\"log-dock collapse collapse-arrow\""

need_pattern "web/app.js" "formatError"
need_pattern "web/app.js" "localizeMessage"
need_pattern "web/app.js" "setup token session is required"
need_pattern "web/app.js" "서버 비밀번호 입력 없이 접속 확인 주소"
need_pattern "web/app.js" "setDoctorPassed"
need_pattern "web/app.js" "setReportReady"
need_pattern "web/app.js" "summaryPanel.hidden = !state.authenticated"
need_pattern "web/app.js" "lucide-static"
need_pattern "web/app.js" "iconMarkup"
need_pattern "web/app.js" "setButtonLabel"
need_pattern "web/app.js" "hydrateIcons"
need_pattern "web/app.js" "confirmInstallStart"
need_pattern "web/app.js" "confirmRecoveryAction"
need_pattern "web/app.js" "completeOperationOverlay"
need_pattern "web/app.js" "초기화 완료되었습니다."
need_pattern "web/app.js" "targetStep: \"login\""
need_pattern "web/app.js" "button.tabIndex = -1"
need_pattern "web/app.js" "setOperationLocked"
need_pattern "web/app.js" "startPackageTicker"
need_pattern "web/app.js" "renderSavedReport"
need_pattern "web/app.js" "bindHelpTooltips"
need_pattern "web/app.js" "설치 검증 실패"
need_pattern "web/app.js" "되돌리기 검증 실패"
need_pattern "web/app.js" "/api/reset"
need_pattern "web/app.js" "/api/rollback"
need_pattern "web/app.js" "/api/recovery"
need_pattern "web/app.js" "restoreWizardState"
need_pattern "web/app.js" "syncServerState"
need_pattern "web/app.js" "x-g7-csrf"
need_pattern "web/app.js" "설치 전 패키지 기준"
need_pattern "web/app.js" "DNS / 네트워크 검증"
need_pattern "web/app.js" "메일 발송 검증"
need_pattern "web/app.js" "SSL / Certbot 검증"
need_pattern "web/app.js" "앱 요구사항"
need_pattern "web/app.js" "planned: \"계획됨\""
need_pattern "web/app.js" "기존 패키지"
need_pattern "web/app.js" "설치 대상"
need_pattern "web/app.js" "packagePurpose"
need_pattern "web/app.js" "캐시, 세션, 큐를 처리하는 Redis 서버입니다."
need_pattern "web/input.css" "whitespace-pre-wrap"
need_pattern "web/input.css" "overflow-wrap: anywhere"
need_pattern "web/input.css" ".app-workspace"
need_pattern "web/input.css" ".workspace-grid"
need_pattern "web/input.css" ".log-dock"
need_pattern "web/input.css" ".operation-overlay"
need_pattern "web/input.css" "g7inst-spin"
need_pattern "web/input.css" "data-status=\"info\""
need_pattern "crates/g7-cli/src/web_setup.rs" "CACHE_CONTROL"
need_pattern "crates/g7-cli/src/web_setup.rs" "emit_progress"
need_pattern "crates/g7-cli/src/web_setup.rs" "ensure_setup_runs_as_root"
need_pattern "crates/g7-cli/src/web_setup.rs" "sudo-token"
need_pattern "crates/g7-cli/src/web_setup.rs" "Server password: not required"

reject_pattern "web/index.html" "login-password|login-username|login-form" "web UI contract regression: password login form must not return"
reject_pattern "web/index.html" "서버 계정|로그인하고 계속|sudo passwd" "web UI contract regression: server account password copy must not return"
reject_pattern "web/index.html" "로컬 테스트|value=\"local-test\"" "web UI contract regression: local-test option must not return to the public wizard"
reject_pattern "web/app.js" "/api/auth/login|login-password|login-username|login-form" "web UI contract regression: password login API must not return"
reject_pattern "crates/g7-cli/src/web_setup.rs" "/api/auth/login|LoginRequest|verify_server_account_password|sudo passwd" "web setup regression: server account password verifier must not return"

if rg -q "installButton\\.textContent|installResultButton\\.textContent|checkNextButton\\.textContent|themeToggle\\.textContent|button\\.textContent" "${ROOT_DIR}/web/app.js"; then
  echo "web UI contract regression: stateful buttons must use setButtonLabel" >&2
  exit 1
fi

if rg -q "로그인 없이 점검만 보기" "${ROOT_DIR}/web/index.html"; then
  echo "web UI contract regression: login must stay as a single clear entry path" >&2
  exit 1
fi

echo "web static smoke passed"
