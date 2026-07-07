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

need_pattern "web/index.html" "G7 설치 마법사"
need_pattern "web/index.html" "data-view=\"login\""
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
need_pattern "web/index.html" "id=\"floating-help\""
need_pattern "web/index.html" "class=\"app-workspace\""
need_pattern "web/index.html" "class=\"workspace-grid\""
need_pattern "web/index.html" "class=\"log-dock collapse collapse-arrow\""

need_pattern "web/app.js" "formatError"
need_pattern "web/app.js" "localizeMessage"
need_pattern "web/app.js" "setDoctorPassed"
need_pattern "web/app.js" "setReportReady"
need_pattern "web/app.js" "summaryPanel.hidden = !state.authenticated"
need_pattern "web/app.js" "lucide-static"
need_pattern "web/app.js" "iconMarkup"
need_pattern "web/app.js" "setButtonLabel"
need_pattern "web/app.js" "hydrateIcons"
need_pattern "web/app.js" "confirmInstallStart"
need_pattern "web/app.js" "confirmRecoveryAction"
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
need_pattern "web/app.js" "기존 보존"
need_pattern "web/app.js" "신규 설치"
need_pattern "web/input.css" "whitespace-pre-wrap"
need_pattern "web/input.css" "overflow-wrap: anywhere"
need_pattern "web/input.css" ".app-workspace"
need_pattern "web/input.css" ".workspace-grid"
need_pattern "web/input.css" ".log-dock"
need_pattern "web/input.css" "data-status=\"info\""
need_pattern "crates/g7-cli/src/web_setup.rs" "CACHE_CONTROL"
need_pattern "crates/g7-cli/src/web_setup.rs" "emit_progress"

if rg -q "installButton\\.textContent|installResultButton\\.textContent|checkNextButton\\.textContent|themeToggle\\.textContent|button\\.textContent" "${ROOT_DIR}/web/app.js"; then
  echo "web UI contract regression: stateful buttons must use setButtonLabel" >&2
  exit 1
fi

if rg -q "로그인 없이 점검만 보기" "${ROOT_DIR}/web/index.html"; then
  echo "web UI contract regression: login must stay as a single clear entry path" >&2
  exit 1
fi

echo "web static smoke passed"
