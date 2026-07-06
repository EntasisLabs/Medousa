#!/usr/bin/env bash
# Stage Tauri bundle artifacts (dmg, msi, exe, AppImage, deb) into dist/.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_TAURI="${1:?crate tauri dir}"
OUT_DIR="${2:?output dir}"
TARGET="${3:-}"

BUNDLE="$("${SCRIPT_DIR}/resolve-tauri-bundle-dir.sh" "${CRATE_TAURI}" "${TARGET}")"
mkdir -p "${OUT_DIR}"

shopt -s nullglob
found=0
while IFS= read -r -d '' file; do
  cp -f "${file}" "${OUT_DIR}/"
  found=1
done < <(find "${BUNDLE}" -type f \( \
  -name '*.dmg' -o \
  -name '*.msi' -o \
  -name '*.exe' -o \
  -name '*.AppImage' -o \
  -name '*.deb' \
  \) -print0)

if [[ "${found}" -eq 0 ]]; then
  echo "::error::bundle dir exists but no installer artifacts found: ${BUNDLE}" >&2
  ls -laR "${BUNDLE}" >&2 || true
  exit 1
fi

echo "staged from ${BUNDLE}:"
ls -la "${OUT_DIR}"
