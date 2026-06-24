#!/usr/bin/env bash
# Build medousa_local (Offline brain) for one Rust target.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
OUTPUT=""
BACKEND="auto"

usage() {
  cat <<'EOF'
Usage: scripts/release/build-local-brain.sh [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --output <dir>        Staging directory (default: dist/build-local-brain/<target>)
  --backend auto|metal|cuda|cpu
  -h, --help            Show this help

Builds medousa_local with embedded inference into <output>/bin/.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --output)
      OUTPUT="$2"
      shift 2
      ;;
    --backend)
      BACKEND="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

medousa_require_cmd cargo
ROOT="$(medousa_repo_root)"
cd "${ROOT}"

if [[ -z "${TARGET}" ]]; then
  TARGET="$(medousa_host_target)"
fi

if [[ -z "${OUTPUT}" ]]; then
  OUTPUT="${ROOT}/dist/build-local-brain/${TARGET}"
fi

resolve_inference_feature() {
  case "${BACKEND}" in
    metal) echo "embedded-inference-metal" ;;
    cuda) echo "embedded-inference-cuda" ;;
    cpu) echo "embedded-inference" ;;
    auto)
      if [[ "${TARGET}" == *-apple-* ]]; then
        echo "embedded-inference-metal"
      else
        echo "embedded-inference"
      fi
      ;;
    *)
      echo "error: unknown backend ${BACKEND}" >&2
      exit 1
      ;;
  esac
}

FEATURE="$(resolve_inference_feature)"
BIN_DIR="${OUTPUT}/bin"
mkdir -p "${BIN_DIR}"

medousa_log "building medousa_local (${FEATURE}) for ${TARGET}"
cargo build --release -p medousa --bin medousa_local --features "${FEATURE}" --target "${TARGET}"

SRC="$(medousa_find_release_binary medousa_local "${TARGET}")"
DST="${BIN_DIR}/$(medousa_binary_filename medousa_local "${TARGET}")"
cp -f "${SRC}" "${DST}"
chmod +x "${DST}" 2>/dev/null || true

VERSION="$(medousa_version)"
cat >"${OUTPUT}/build-meta.env" <<EOF
MEDOUSA_VERSION=${VERSION}
MEDOUSA_TARGET=${TARGET}
MEDOUSA_BACKEND=${BACKEND}
MEDOUSA_BIN_DIR=${BIN_DIR}
EOF

medousa_log "done — ${DST}"
