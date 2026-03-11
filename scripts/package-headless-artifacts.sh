#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
BIN_PATH="${1:-${ROOT_DIR}/src-tauri/target/release/ai-open-router}"

if [[ ! -f "${BIN_PATH}" ]]; then
  echo "headless binary not found: ${BIN_PATH}"
  exit 1
fi

platform="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

case "${platform}" in
  darwin) platform="darwin" ;;
  linux) platform="linux" ;;
  msys*|mingw*|cygwin*) platform="windows" ;;
  *) echo "unsupported platform: ${platform}" ; exit 1 ;;
esac

case "${arch}" in
  x86_64) arch="x64" ;;
  aarch64|arm64) arch="arm64" ;;
  *) echo "unsupported arch: ${arch}" ; exit 1 ;;
esac

mkdir -p "${DIST_DIR}"

asset_name="ai-open-router-${platform}-${arch}.tar.gz"
zip_name="ai-open-router-${platform}-${arch}.zip"
raw_name="ai-open-router-${platform}-${arch}"
tmp_dir="$(mktemp -d)"

bin_name="ai-open-router"
if [[ "${platform}" == "windows" ]]; then
  bin_name="ai-open-router.exe"
  raw_name="${raw_name}.exe"
fi

cp -f "${BIN_PATH}" "${tmp_dir}/${bin_name}"
tar -czf "${DIST_DIR}/${asset_name}" -C "${tmp_dir}" "${bin_name}"
cp -f "${tmp_dir}/${bin_name}" "${DIST_DIR}/${raw_name}"

PYTHON_BIN=""
if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN="python"
fi

if [[ -n "${PYTHON_BIN}" ]]; then
  "${PYTHON_BIN}" - <<PY
import zipfile
from pathlib import Path

dist = Path(r"${DIST_DIR}")
zip_path = dist / "${zip_name}"
bin_path = dist / "${raw_name}"

with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    zf.write(bin_path, arcname="${bin_name}")
PY
else
  echo "python not found; skipping zip packaging"
fi

rm -rf "${tmp_dir}"

echo "headless artifact: ${DIST_DIR}/${asset_name}"
echo "headless artifact: ${DIST_DIR}/${raw_name}"
if [[ -f "${DIST_DIR}/${zip_name}" ]]; then
  echo "headless artifact: ${DIST_DIR}/${zip_name}"
fi
