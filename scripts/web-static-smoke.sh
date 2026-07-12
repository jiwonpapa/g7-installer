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

need_web_setup_pattern() {
  local pattern="$1"

  if ! rg -q "${pattern}" "${ROOT_DIR}/crates/g7-cli/src/web_setup.rs" "${ROOT_DIR}/crates/g7-cli/src/web_setup"; then
    echo "missing web UI contract in web setup source: ${pattern}" >&2
    exit 1
  fi
}

reject_web_setup_pattern() {
  local pattern="$1"
  local message="$2"

  if rg -q "${pattern}" "${ROOT_DIR}/crates/g7-cli/src/web_setup.rs" "${ROOT_DIR}/crates/g7-cli/src/web_setup"; then
    echo "${message}" >&2
    exit 1
  fi
}

need_install_source_pattern() {
  local pattern="$1"

  if ! rg -q "${pattern}" "${ROOT_DIR}/crates/g7-core/src/commands/install.rs" "${ROOT_DIR}/crates/g7-core/src/commands/install"; then
    echo "missing install contract in install source: ${pattern}" >&2
    exit 1
  fi
}

reject_install_source_pattern() {
  local pattern="$1"
  local message="$2"

  if rg -q "${pattern}" "${ROOT_DIR}/crates/g7-core/src/commands/install.rs" "${ROOT_DIR}/crates/g7-core/src/commands/install"; then
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
need_pattern "web/index.html" "data-view=\"provision\""
need_pattern "web/index.html" "id=\"theme-toggle\""
need_pattern "web/index.html" "class=\"btn btn-sm btn-outline icon-button\""
need_pattern "web/index.html" "data-icon=\"package-plus\""
need_pattern "web/index.html" "data-ui-icon=\"shield-check\""
need_pattern "web/index.html" "id=\"install-progress\""
need_pattern "web/index.html" "id=\"install-progress-dialog\""
need_pattern "web/index.html" "id=\"package-progress-list\""
need_pattern "web/index.html" "id=\"install-result-button\""
need_pattern "web/index.html" "id=\"report-progress\""
need_pattern "web/index.html" "id=\"check-next-button\""
need_pattern "web/index.html" "계획 새로고침"
need_pattern "web/index.html" "선택한 사양을 바탕으로 설치 계획을 자동 생성합니다."
need_pattern "web/index.html" "name=\"app_package\""
need_pattern "web/index.html" "name=\"database_version\""
need_pattern "web/index.html" "value=\"8.3\" selected"
need_pattern "web/index.html" "name=\"stack_profile\" value=\"stable\" checked"
need_pattern "web/index.html" "name=\"stack_profile\" value=\"latest\""
need_pattern "web/index.html" "id=\"stack-web-product\""
need_pattern "web/index.html" "id=\"brand-nginx\""
need_pattern "web/index.html" "value=\"8.0\" selected"
need_pattern "web/index.html" "value=\"8.4\""
reject_pattern "web/index.html" "mariadb" "public wizard must not expose MariaDB"
need_pattern "web/index.html" "value=\"redirect-to-www\" selected"
need_pattern "web/index.html" "value=\"none\" selected"
need_pattern "web/index.html" "id=\"smtp-password-confirm\""
need_pattern "web/index.html" "id=\"operation-overlay-log-slot\""
need_pattern "web/index.html" "name=\"database_name\""
need_pattern "web/index.html" "name=\"database_user\""
need_pattern "web/index.html" "name=\"database_password\""
need_pattern "web/index.html" "id=\"provision-action-dialog\""
need_pattern "web/index.html" "승인하고 적용/점검"
need_pattern "web/index.html" "세부 설정 적용/점검"
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
need_pattern "web/index.html" "id=\"live-log\""

need_pattern "web/app.js" "formatError"
need_pattern "web/app.js" "localizeMessage"
need_pattern "web/app.js" "setup token session is required"
need_pattern "web/app.js" "서버 비밀번호 입력 없이 접속 확인 주소"
need_pattern "web/app.js" "setDoctorPassed"
need_pattern "web/app.js" "applyStackProfile"
need_pattern "web/app.js" "stackProfileForVersions"
need_pattern "web/app.js" "refreshStackPreview"
need_pattern "web/app.js" "doctor-overview"
need_pattern "web/app.js" "setReportReady"
need_pattern "web/app.js" "const shouldShowSummary = Boolean"
need_pattern "web/app.js" "summaryPanel.hidden = !shouldShowSummary"
need_pattern "web/app.js" "document.body.dataset.activeStep = step"
need_pattern "web/app.js" "lucide-static"
need_pattern "web/app.js" "iconMarkup"
need_pattern "web/app.js" "setButtonLabel"
need_pattern "web/app.js" "hydrateIcons"
need_pattern "web/app.js" "confirmInstallStart"
need_pattern "web/app.js" "confirmRecoveryAction"
need_pattern "web/app.js" "completeOperationOverlay"
need_pattern "web/app.js" "operationOverlayLogSlot"
need_pattern "web/app.js" "nodes.logDock.open = true"
need_pattern "web/app.js" "초기화 완료되었습니다."
need_pattern "web/app.js" "targetStep: \"login\""
need_pattern "web/app.js" "button.tabIndex = -1"
need_pattern "web/app.js" "setOperationLocked"
need_pattern "web/app.js" "startPackageTicker"
need_pattern "web/app.js" "completePendingPackageProgress"
need_pattern "web/app.js" "renderSavedReport"
need_pattern "web/app.js" "reportFailureCard"
need_pattern "web/app.js" "중단 원인"
need_pattern "web/app.js" "bindHelpTooltips"
need_pattern "web/app.js" "설치 검증 실패"
need_pattern "web/app.js" "되돌리기 검증 실패"
need_pattern "web/app.js" "/api/reset"
need_pattern "web/app.js" "/api/rollback"
need_pattern "web/app.js" "/api/recovery"
need_pattern "web/app.js" "restoreWizardState"
need_pattern "web/app.js" "g7inst-wizard-state-v2"
need_pattern "web/app.js" "syncServerState"
need_pattern "web/app.js" "beforeunload"
need_pattern "web/app.js" "navigationGuardActive"
need_pattern "web/app.js" "warnNavigationBlocked"
need_pattern "web/app.js" "setInstallRunning"
need_pattern "web/app.js" "generatePlan\\(\\{ auto: true \\}\\)"
need_pattern "web/app.js" "사양 확인 준비 완료"
need_pattern "web/app.js" "맞으면 진행하고, 다르면 이전으로 돌아가 수정하세요."
need_pattern "web/app.js" "stepRoutes"
need_pattern "web/app.js" "/setup/provision"
need_pattern "web/app.js" "databaseError"
need_pattern "web/app.js" "renderProvisionPanel"
need_pattern "web/app.js" "openProvisionActionDialog"
need_pattern "web/app.js" "runProvisionAction"
need_pattern "web/app.js" "completionStateCard"
need_pattern "web/app.js" "reportDownloadCard"
need_pattern "web/app.js" "downloadReport"
need_pattern "web/app.js" "복구 매니페스트"
need_pattern "web/app.js" "보안 경계 안내"
need_pattern "web/app.js" "modules/event-stream.js"
need_pattern "web/modules/event-stream.js" "connectEventStream"
reject_pattern "web/app.js" "ufw status" "web UI must not execute UFW operations"
need_pattern "web/app.js" "package-plan-list"
need_pattern "web/app.js" "package-plan-group"
need_pattern "web/app.js" "planAccordionSummary"
need_pattern "web/app.js" "finishInstallProgressDialog"
need_pattern "web/app.js" "x-g7-csrf"
need_pattern "web/app.js" "세부 설정 카드"
need_pattern "web/app.js" "DB 비밀번호"
need_pattern "web/app.js" "SSL/Certbot"
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
need_pattern "web/input.css" "body\\[data-active-step=\"install\"\\] \\.promo-panel"
need_pattern "web/input.css" "body\\[data-active-step=\"login\"\\] \\[data-progress=\"login\"\\]"
need_pattern "web/input.css" ".plan-review"
need_pattern "web/input.css" ".package-plan-list"
need_pattern "web/input.css" "lg:grid-cols-2"
need_pattern "web/input.css" ".operation-overlay"
need_pattern "web/input.css" ".provision-detail-grid"
need_pattern "web/input.css" ".provision-modal-box"
need_pattern "web/input.css" ".download-actions"
need_pattern "web/input.css" "g7inst-spin"
need_pattern "web/input.css" "data-status=\"info\""
need_web_setup_pattern "CACHE_CONTROL"
need_web_setup_pattern "emit_progress"
need_web_setup_pattern "ensure_setup_runs_as_root_or_reexec"
need_web_setup_pattern "reexec_setup_with_sudo"
need_web_setup_pattern "sudo-token"
need_web_setup_pattern "Server password: handled in SSH/root shell before this controller starts"
need_web_setup_pattern "/setup/provision"
need_web_setup_pattern "validate_database_request"
need_web_setup_pattern "install_running: state.install_running.load"
need_web_setup_pattern "provision_security"
need_install_source_pattern "BACKUP_MANIFEST_PATH"
need_install_source_pattern "backup_manifest_content"
need_install_source_pattern "postfix_preseed"
need_install_source_pattern "inet_interfaces"
need_pattern "crates/g7-system/src/mail.rs" "debconf-set-selections"
need_pattern "crates/g7-system/src/mail.rs" "postconf"
need_pattern ".github/workflows/quality-gate.yml" "Browser wizard E2E"
need_pattern "scripts/web-ui-e2e.spec.mjs" "wizard routes render report"

reject_pattern "web/index.html" "login-password|login-username|login-form" "web UI contract regression: password login form must not return"
reject_pattern "web/index.html" "서버 계정|로그인하고 계속|sudo passwd" "web UI contract regression: server account password copy must not return"
reject_pattern "web/index.html" "로컬 테스트|value=\"local-test\"" "web UI contract regression: local-test option must not return to the public wizard"
reject_pattern "web/index.html" "옵션을 확인한 뒤 계획 생성을 누르세요" "web UI contract regression: plan step must auto-generate the review"
reject_pattern "web/index.html" "최근 로그|install-live-log|activity-log-count" "web UI contract regression: duplicate install log panel must not return"
reject_pattern "web/app.js" "/api/auth/login|login-password|login-username|login-form" "web UI contract regression: password login API must not return"
reject_pattern "web/app.js" "계획 생성 버튼을 누르면 실제 plan 결과로 교체됩니다" "web UI contract regression: manual-only plan copy must not return"
reject_pattern "web/app.js" "검증 대기 100%|percent \\+=|각 패키지의 진행률" "web UI contract regression: fake per-package percentages must not return"
reject_web_setup_pattern "/api/auth/login|LoginRequest|verify_server_account_password|sudo passwd" "web setup regression: server account password verifier must not return"
reject_install_source_pattern "access_log /var/log/nginx/g7-access.log g7_timing|log_format g7_timing" "nginx config regression: installer vhost must not depend on global custom log_format"

if rg -q "installButton\\.textContent|installResultButton\\.textContent|checkNextButton\\.textContent|themeToggle\\.textContent|button\\.textContent" "${ROOT_DIR}/web/app.js"; then
  echo "web UI contract regression: stateful buttons must use setButtonLabel" >&2
  exit 1
fi

if rg -q "로그인 없이 점검만 보기" "${ROOT_DIR}/web/index.html"; then
  echo "web UI contract regression: login must stay as a single clear entry path" >&2
  exit 1
fi

echo "web static smoke passed"
