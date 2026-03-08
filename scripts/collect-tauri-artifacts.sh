#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
TARGET_DIRS=(
  "${ROOT_DIR}/dist/target"
  "${ROOT_DIR}/src-tauri/dist/target"
  "${ROOT_DIR}/src-tauri/target"
)

mkdir -p "${DIST_DIR}"
shopt -s nullglob

copy_artifact() {
  local source_file="$1"
  [[ -f "${source_file}" ]] || return 0
  cp -f "${source_file}" "${DIST_DIR}/"
  echo "copied: $(basename "${source_file}")"
}

for target_dir in "${TARGET_DIRS[@]}"; do
  for file in "${target_dir}"/release/bundle/dmg/*.dmg; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/deb/*.deb; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/nsis/*-setup.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/*/*/release/bundle/nsis/*-setup.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router-tauri.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router-tauri; do
    copy_artifact "${file}"
  done
done

echo "tauri artifacts synced to ${DIST_DIR}"
