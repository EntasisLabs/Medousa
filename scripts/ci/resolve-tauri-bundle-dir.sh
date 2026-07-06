#!/usr/bin/env bash
# Resolve Tauri bundle output directory (native vs --target triple layouts).
set -euo pipefail

CRATE_TAURI="${1:?crate tauri dir required}"
TARGET="${2:-}"
HOST="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)"

candidates=()
if [[ -n "${TARGET}" ]]; then
  candidates+=("${CRATE_TAURI}/target/${TARGET}/release/bundle")
fi
candidates+=("${CRATE_TAURI}/target/release/bundle")
if [[ -n "${HOST}" && "${HOST}" != "${TARGET}" ]]; then
  candidates+=("${CRATE_TAURI}/target/${HOST}/release/bundle")
fi

for dir in "${candidates[@]}"; do
  if [[ -d "${dir}" ]]; then
    echo "${dir}"
    exit 0
  fi
done

echo "::error::Tauri bundle directory not found under ${CRATE_TAURI}/target" >&2
echo "Searched:" >&2
printf '  %s\n' "${candidates[@]}" >&2
if [[ -d "${CRATE_TAURI}/target" ]]; then
  echo "target layout:" >&2
  find "${CRATE_TAURI}/target" -maxdepth 4 -type d 2>/dev/null | head -50 >&2 || true
fi
exit 1
