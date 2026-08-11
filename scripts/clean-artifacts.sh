#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DRY_RUN=1
KEEP_RELEASE=0
KEEP_WEB_DEPS=0
QUIET=0

usage() {
  cat <<'USAGE'
Usage: scripts/clean-artifacts.sh [--dry-run] [--yes] [--keep-release] [--keep-web-deps] [--quiet]

Removes only project-owned build/test artifacts. It never uses broad git clean,
so ignored secrets such as .env, keys, and *.pem are not touched.
USAGE
}

for arg in "$@"; do
  case "${arg}" in
    --dry-run) DRY_RUN=1 ;;
    --yes) DRY_RUN=0 ;;
    --keep-release) KEEP_RELEASE=1 ;;
    --keep-web-deps) KEEP_WEB_DEPS=1 ;;
    --quiet) QUIET=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: ${arg}" >&2
      usage >&2
      exit 2
      ;;
  esac
done

say() {
  if [[ "${QUIET}" != "1" ]]; then
    echo "$@"
  fi
}

remove_path() {
  local rel="$1"
  local path="${ROOT_DIR}/${rel}"

  case "${path}" in
    "${ROOT_DIR}"/*) ;;
    *)
      echo "refusing to remove path outside repo: ${path}" >&2
      exit 1
      ;;
  esac

  if [[ ! -e "${path}" ]]; then
    return
  fi

  if [[ "${DRY_RUN}" == "1" ]]; then
    say "[clean-artifacts] would remove ${rel}"
  else
    say "[clean-artifacts] remove ${rel}"
    rm -rf "${path}"
  fi
}

remove_find_match() {
  local path="$1"
  local rel="${path#${ROOT_DIR}/}"

  if [[ "${path}" == "${ROOT_DIR}" ]]; then
    return
  fi
  case "${path}" in
    "${ROOT_DIR}"/*) ;;
    *)
      echo "refusing to remove path outside repo: ${path}" >&2
      exit 1
      ;;
  esac

  if [[ "${DRY_RUN}" == "1" ]]; then
    say "[clean-artifacts] would remove ${rel}"
  else
    say "[clean-artifacts] remove ${rel}"
    rm -rf "${path}"
  fi
}

remove_path "target"
remove_path "coverage"
remove_path "test-results"
remove_path "playwright-report"
remove_path "web/test-results"
remove_path "web/playwright-report"

if [[ "${KEEP_RELEASE}" != "1" ]]; then
  remove_path "dist"
fi

if [[ "${KEEP_WEB_DEPS}" != "1" ]]; then
  remove_path "web/node_modules"
fi

while IFS= read -r path; do
  remove_find_match "${path}"
done < <(
  find "${ROOT_DIR}" \
    -path "${ROOT_DIR}/.git" -prune -o \
    -path "${ROOT_DIR}/web/node_modules" -prune -o \
    -type d \( -name __pycache__ -o -name .pytest_cache \) -print
)

while IFS= read -r path; do
  remove_find_match "${path}"
done < <(
  find "${ROOT_DIR}" \
    -path "${ROOT_DIR}/.git" -prune -o \
    -path "${ROOT_DIR}/web/node_modules" -prune -o \
    -type f \( -name "*.profraw" -o -name "*.profdata" -o -name "tarpaulin-report.html" -o -name "lcov.info" \) -print
)

if [[ "${DRY_RUN}" == "1" ]]; then
  say "[clean-artifacts] dry-run only. Re-run with --yes to delete."
fi
