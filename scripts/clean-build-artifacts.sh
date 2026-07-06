#!/usr/bin/env bash
# Remove local Rust/JS build artifacts from the Medousa tree.
# Safe to run anytime — next build is a full recompile.
#
# Usage:
#   ./scripts/clean-build-artifacts.sh          # in-tree targets + dist staging
#   ./scripts/clean-build-artifacts.sh --cache  # also wipe ../.cache/cargo-target
#   ./scripts/clean-build-artifacts.sh --all    # --cache + node_modules

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_PARENT="$(cd "${ROOT}/.." && pwd)"
CACHE_DIR="${MEDOUSA_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-${REPO_PARENT}/.cache/cargo-target}}"

INCLUDE_CACHE=0
INCLUDE_NODE=0
for arg in "$@"; do
  case "${arg}" in
    --cache) INCLUDE_CACHE=1 ;;
    --all) INCLUDE_CACHE=1; INCLUDE_NODE=1 ;;
    -h|--help)
      sed -n '2,8p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: ${arg} (try --help)" >&2
      exit 1
      ;;
  esac
done

removed=()

rm_rf() {
  local path="$1"
  if [[ -e "${path}" ]]; then
    local size
    size="$(du -sh "${path}" 2>/dev/null | cut -f1 || echo "?")"
    echo "removing ${path} (${size})"
    rm -rf "${path}"
    removed+=("${path} (${size})")
  fi
}

echo "medousa clean: root=${ROOT}"

rm_rf "${ROOT}/target"
rm_rf "${ROOT}/apps/medousa-home/src-tauri/target"
rm_rf "${ROOT}/apps/medousa-installer/src-tauri/target"
rm_rf "${ROOT}/adapters/medousa-whatsapp/target"
rm_rf "${ROOT}/.cargo-target"
rm_rf "${ROOT}/target-local"
rm_rf "${ROOT}/adapters/medousa-whatsapp/.cargo-target"
rm_rf "${ROOT}/dist/build"
rm_rf "${ROOT}/dist/build-local-brain"
rm_rf "${REPO_PARENT}/medousa_local-auto-"*
rm_rf "${ROOT}/apps/medousa-home/src-tauri/gen/apple/build"
rm_rf "${ROOT}/apps/medousa-home/src-tauri/gen/apple/Externals"
rm_rf "${ROOT}/apps/medousa-home/src-tauri/binaries"

if [[ "${INCLUDE_CACHE}" -eq 1 ]]; then
  rm_rf "${CACHE_DIR}"
fi

if [[ "${INCLUDE_NODE}" -eq 1 ]]; then
  rm_rf "${ROOT}/apps/medousa-home/node_modules"
  rm_rf "${ROOT}/apps/medousa-installer/node_modules"
  rm_rf "${ROOT}/apps/medousa-home/.svelte-kit"
fi

if [[ "${#removed[@]}" -eq 0 ]]; then
  echo "nothing to remove (already clean)"
else
  echo "removed ${#removed[@]} path(s):"
  printf '  - %s\n' "${removed[@]}"
fi

echo "active cargo target-dir: ${CACHE_DIR}"
echo "next: cargo build -p medousa --bin medousa_daemon  (artifacts land in cache, not Medousa/)"
