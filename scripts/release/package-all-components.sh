#!/usr/bin/env bash
# Package component tarballs (optionally filtered) plus the full-suite archive.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
INPUT=""
DIST_DIR=""
# Comma-separated package ids, or empty = all MEDOUSA_COMPONENT_IDS
PACKAGES=""
SKIP_SUITE=0

usage() {
  cat <<'EOF'
Usage: scripts/release/package-all-components.sh [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --input <dir>         Staging dir from build.sh
  --dist <dir>          Output directory (default: dist/)
  --packages <list>     Comma-separated package ids (default: all components)
  --skip-suite          Do not build the full medousa-v* suite tarball
  -h, --help            Show this help

Packages engine, cli, each adapter, mcp-gateway (or a subset), and optionally
the full medousa-v* suite tarball.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --input) INPUT="$2"; shift 2 ;;
    --dist) DIST_DIR="$2"; shift 2 ;;
    --packages) PACKAGES="$2"; shift 2 ;;
    --skip-suite) SKIP_SUITE=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ARGS=()
[[ -n "${TARGET}" ]] && ARGS+=(--target "${TARGET}")
[[ -n "${INPUT}" ]] && ARGS+=(--input "${INPUT}")
[[ -n "${DIST_DIR}" ]] && ARGS+=(--dist "${DIST_DIR}")

PACKAGE_LIST=()
if [[ -n "${PACKAGES}" ]]; then
  IFS=',' read -r -a PACKAGE_LIST <<<"${PACKAGES}"
else
  PACKAGE_LIST=("${MEDOUSA_COMPONENT_IDS[@]}")
fi

for package_id in "${PACKAGE_LIST[@]}"; do
  package_id="$(echo "${package_id}" | xargs)"
  [[ -n "${package_id}" ]] || continue
  "${SCRIPT_DIR}/package-component.sh" --package "${package_id}" "${ARGS[@]}"
done

if [[ "${SKIP_SUITE}" -eq 0 ]]; then
  # Suite only when packaging the full component set (or explicitly engine+cli…).
  # Skip when a narrow targeted list omits most bins.
  if [[ -z "${PACKAGES}" ]]; then
    "${SCRIPT_DIR}/package.sh" "${ARGS[@]}"
  fi
fi
