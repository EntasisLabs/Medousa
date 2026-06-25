#!/usr/bin/env bash
# Package all component tarballs plus the full-suite medousa-v* archive for install.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
INPUT=""
DIST_DIR=""

usage() {
  cat <<'EOF'
Usage: scripts/release/package-all-components.sh [options]

Options:
  --target <triple>   Rust target triple (default: host)
  --input <dir>       Staging dir from build.sh
  --dist <dir>        Output directory (default: dist/)
  -h, --help          Show this help

Packages engine, cli, each adapter, mcp-gateway, and the full medousa-v* suite tarball.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --input) INPUT="$2"; shift 2 ;;
    --dist) DIST_DIR="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ARGS=()
[[ -n "${TARGET}" ]] && ARGS+=(--target "${TARGET}")
[[ -n "${INPUT}" ]] && ARGS+=(--input "${INPUT}")
[[ -n "${DIST_DIR}" ]] && ARGS+=(--dist "${DIST_DIR}")

for package_id in "${MEDOUSA_COMPONENT_IDS[@]}"; do
  "${SCRIPT_DIR}/package-component.sh" --package "${package_id}" "${ARGS[@]}"
done

"${SCRIPT_DIR}/package.sh" "${ARGS[@]}"
