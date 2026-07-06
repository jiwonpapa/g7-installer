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
need_pattern "web/index.html" "id=\"install-progress\""
need_pattern "web/index.html" "id=\"reset-button\""
need_pattern "web/index.html" "id=\"rollback-button\""

need_pattern "web/app.js" "formatError"
need_pattern "web/app.js" "localizeMessage"
need_pattern "web/app.js" "설치 검증 실패"
need_pattern "web/app.js" "되돌리기 검증 실패"
need_pattern "web/app.js" "/api/reset"
need_pattern "web/app.js" "/api/rollback"
need_pattern "web/app.js" "x-g7-csrf"

echo "web static smoke passed"
