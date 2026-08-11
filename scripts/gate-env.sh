#!/usr/bin/env bash
# Shared local gate helpers. Keep this file dependency-free and source-only.

g7_default_cargo_target_dir() {
  if [[ -n "${G7_CARGO_CACHE_DIR:-}" ]]; then
    echo "${G7_CARGO_CACHE_DIR}"
    return
  fi

  if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
    echo "${XDG_CACHE_HOME}/g7-installer/cargo-target"
    return
  fi

  if [[ -n "${HOME:-}" && -d "${HOME}/Library/Caches" ]]; then
    echo "${HOME}/Library/Caches/g7-installer/cargo-target"
    return
  fi

  echo "${HOME:-/tmp}/.cache/g7-installer/cargo-target"
}

g7_prepare_cargo_target() {
  local label="$1"
  local explicit_dir="${2:-}"

  G7_TEMP_CARGO_TARGET_CREATED=0
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "[${label}] cargo target: ${CARGO_TARGET_DIR}"
    export CARGO_TARGET_DIR
    return
  fi

  if [[ -n "${explicit_dir}" ]]; then
    CARGO_TARGET_DIR="${explicit_dir}"
    echo "[${label}] cargo target: ${CARGO_TARGET_DIR}"
    export CARGO_TARGET_DIR
    return
  fi

  if [[ "${G7_USE_TEMP_TARGET:-0}" == "1" ]]; then
    CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/g7inst-${label}-target.XXXXXX")"
    G7_TEMP_CARGO_TARGET_CREATED=1
    export CARGO_TARGET_DIR
    echo "[${label}] isolated cargo target: ${CARGO_TARGET_DIR}"
    return
  fi

  CARGO_TARGET_DIR="$(g7_default_cargo_target_dir)"
  mkdir -p "${CARGO_TARGET_DIR}"
  export CARGO_TARGET_DIR
  echo "[${label}] cargo target cache: ${CARGO_TARGET_DIR}"
}

g7_prepare_temp_cargo_target() {
  g7_prepare_cargo_target "$@"
}

g7_cleanup_temp_cargo_target() {
  local keep="${1:-0}"

  if [[ "${G7_TEMP_CARGO_TARGET_CREATED:-0}" == "1" && "${keep}" != "1" ]]; then
    rm -rf "${CARGO_TARGET_DIR}"
  fi
}
