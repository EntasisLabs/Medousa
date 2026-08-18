#!/usr/bin/env bash
# Unused-direct-dep scan for first-party workspace crates (not vendor / Tauri apps).
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

if ! command -v cargo-machete >/dev/null 2>&1 && ! cargo machete --version >/dev/null 2>&1; then
  echo "check-unused-deps: cargo-machete is not installed" >&2
  exit 1
fi

paths=()
while IFS= read -r dir; do
  paths+=("${dir}")
done < <(find crates adapters -mindepth 1 -maxdepth 1 -type d | sort)

cargo machete --with-metadata "${paths[@]}"
