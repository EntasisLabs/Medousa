#!/usr/bin/env bash
# Build all Medousa release binaries for one Rust target into a staging directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
OUTPUT=""
PRINT_TARGET_ONLY=0

usage() {
  cat <<'EOF'
Usage: scripts/release/build.sh [options]

Options:
  --target <triple>   Rust target triple (default: host)
  --output <dir>      Staging directory (default: dist/build/<target>)
  --print-target      Print resolved target triple and exit
  -h, --help          Show this help

Builds root workspace binaries + medousa_whatsapp, copies into <output>/bin/.
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
    --print-target)
      PRINT_TARGET_ONLY=1
      shift
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
medousa_require_cmd rustc

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

if [[ -z "${TARGET}" ]]; then
  TARGET="$(medousa_host_target)"
fi

if [[ "${PRINT_TARGET_ONLY}" -eq 1 ]]; then
  echo "${TARGET}"
  exit 0
fi

if [[ -z "${OUTPUT}" ]]; then
  OUTPUT="${ROOT}/dist/build/${TARGET}"
fi

BIN_DIR="${OUTPUT}/bin"
mkdir -p "${BIN_DIR}"

medousa_assert_versions_match
VERSION="$(medousa_version)"

medousa_log "building medousa v${VERSION} for ${TARGET}"
medousa_log "staging → ${BIN_DIR}"

CARGO_TARGET_ARGS=()
if [[ -n "${TARGET}" ]]; then
  CARGO_TARGET_ARGS=(--target "${TARGET}")
fi

medousa_log "cargo build (root workspace, release, all bins)…"
cargo build --release --bins "${CARGO_TARGET_ARGS[@]}"

medousa_log "cargo build (medousa_whatsapp)…"
cargo build --release --manifest-path "${MEDOUSA_WHATSAPP_MANIFEST}" "${CARGO_TARGET_ARGS[@]}"

MAIN_RELEASE="$(medousa_cargo_release_dir "${TARGET}")"
WA_RELEASE="$(medousa_whatsapp_cargo_release_dir "${TARGET}")"

for bin in "${MEDOUSA_BINARIES[@]}"; do
  src=""
  if [[ "${bin}" == "medousa_whatsapp" ]]; then
    src="$(medousa_find_release_binary "${bin}" "${TARGET}" || true)"
  else
    src="$(medousa_find_release_binary "${bin}" "${TARGET}" || true)"
  fi
  if [[ -z "${src}" || ! -f "${src}" ]]; then
    echo "error: expected binary missing: ${bin} (searched under ${MAIN_RELEASE} and ${WA_RELEASE})" >&2
    exit 1
  fi
  dst="${BIN_DIR}/$(medousa_binary_filename "${bin}" "${TARGET}")"
  cp -f "${src}" "${dst}"
  chmod +x "${dst}" 2>/dev/null || true
  medousa_log "  ${bin}"
done

# Metadata for package step
cat >"${OUTPUT}/build-meta.env" <<EOF
MEDOUSA_VERSION=${VERSION}
MEDOUSA_TARGET=${TARGET}
MEDOUSA_BIN_DIR=${BIN_DIR}
EOF

medousa_log "done — $(find "${BIN_DIR}" -type f | wc -l | tr -d ' ') binaries in ${BIN_DIR}"
