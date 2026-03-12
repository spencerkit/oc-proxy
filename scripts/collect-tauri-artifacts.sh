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
  for file in "${target_dir}"/release/bundle/macos/*.app.tar.gz; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/macos/*.app.tar.gz.sig; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/deb/*.deb; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/appimage/*.AppImage; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/appimage/*.AppImage.sig; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/nsis/*-setup.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/nsis/*-setup.exe.sig; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/*/*/release/bundle/nsis/*-setup.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/*/*/release/bundle/nsis/*-setup.exe.sig; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/msi/*.msi; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/bundle/msi/*.msi.sig; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router-tauri.exe; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router-tauri; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router; do
    copy_artifact "${file}"
  done
  for file in "${target_dir}"/release/ai-open-router.exe; do
    copy_artifact "${file}"
  done
  while IFS= read -r -d '' file; do
    copy_artifact "${file}"
  done < <(find "${target_dir}/release" -name "latest.json" -print0 2>/dev/null || true)
  while IFS= read -r -d '' file; do
    copy_artifact "${file}"
  done < <(find "${target_dir}/release" -name "latest.json.sig" -print0 2>/dev/null || true)
done

echo "tauri artifacts synced to ${DIST_DIR}"
