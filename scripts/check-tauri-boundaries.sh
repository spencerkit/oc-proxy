#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

fail=0

check_forbidden() {
  local title="$1"
  local search_root="$2"
  local pattern="$3"

  local output
  output="$(rg -n --glob '*.rs' "$pattern" "$search_root" || true)"
  if [[ -n "$output" ]]; then
    echo "[boundary-check] FAIL: $title"
    echo "$output"
    fail=1
  fi
}

check_forbidden \
  "commands must not depend on infrastructure modules directly" \
  "src-tauri/src/commands" \
  '^\s*use\s+crate::(proxy|quota|remote_sync)::'

check_forbidden \
  "services must not depend on commands layer" \
  "src-tauri/src/services" \
  '^\s*use\s+crate::commands::'

check_forbidden \
  "config modules must not depend on command/service layers" \
  "src-tauri/src/config" \
  '^\s*use\s+crate::(commands|services)::'

if [[ "$fail" -ne 0 ]]; then
  echo "[boundary-check] Failed"
  exit 1
fi

echo "[boundary-check] OK"
