#!/usr/bin/env bash
# Package a single Medousa component (engine, adapter, cli, etc.) into a release tarball.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

PACKAGE_ID=""
TARGET=""
INPUT=""
DIST_DIR=""

usage() {
  cat <<'EOF'
Usage: scripts/release/package-component.sh [options]

Options:
  --package <id>      Component package id (engine, cli, adapter-telegram, …)
  --target <triple>   Rust target triple (default: host)
  --input <dir>       Staging dir from build.sh (contains bin/)
  --dist <dir>        Output directory (default: dist/)
  -h, --help          Show this help

Creates dist/<package>-vX.Y.Z-<target>.tar.gz and appends to dist/SHA256SUMS.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) PACKAGE_ID="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --input) INPUT="$2"; shift 2 ;;
    --dist) DIST_DIR="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

[[ -n "${PACKAGE_ID}" ]] || {
  echo "error: --package is required" >&2
  exit 1
}

medousa_require_cmd tar

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

if [[ "${PACKAGE_ID}" == "adapter-whatsapp" ]]; then
  medousa_assert_whatsapp_package_version
fi
VERSION="$(medousa_package_version "${PACKAGE_ID}")"

if [[ -z "${TARGET}" ]]; then
  if [[ -n "${INPUT}" && -f "${INPUT}/build-meta.env" ]]; then
    # shellcheck disable=SC1090
    source "${INPUT}/build-meta.env"
    TARGET="${MEDOUSA_TARGET:-}"
  fi
  if [[ -z "${TARGET}" ]]; then
    TARGET="$(medousa_host_target)"
  fi
fi

if [[ -z "${INPUT}" ]]; then
  INPUT="${ROOT}/dist/build/${TARGET}"
fi

if [[ -z "${DIST_DIR}" ]]; then
  DIST_DIR="${ROOT}/dist"
fi

BIN_DIR="${INPUT}/bin"
if [[ ! -d "${BIN_DIR}" ]]; then
  echo "error: missing bin directory: ${BIN_DIR} (run build.sh first)" >&2
  exit 1
fi

mkdir -p "${DIST_DIR}"

ARCHIVE_NAME="$(medousa_component_archive_name "${PACKAGE_ID}" "${VERSION}" "${TARGET}")"
ARCHIVE_PATH="${DIST_DIR}/${ARCHIVE_NAME}"
BASENAME="$(medousa_component_basename "${PACKAGE_ID}" "${VERSION}" "${TARGET}")"
CHECKSUMS_FILE="${DIST_DIR}/SHA256SUMS"

medousa_log "packaging component ${PACKAGE_ID} → ${ARCHIVE_NAME}"

WORK="${DIST_DIR}/.pack-work-${BASENAME}"
rm -rf "${WORK}"
mkdir -p "${WORK}/${BASENAME}/bin"

read -r -a COMPONENT_BINS <<<"$(medousa_component_binaries "${PACKAGE_ID}")"
for bin in "${COMPONENT_BINS[@]}"; do
  file="$(medousa_binary_filename "${bin}" "${TARGET}")"
  src="${BIN_DIR}/${file}"
  [[ -f "${src}" ]] || {
    echo "error: missing binary for ${PACKAGE_ID}: ${src}" >&2
    exit 1
  }
  cp -a "${src}" "${WORK}/${BASENAME}/bin/${file}"
done

BUILT_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
medousa_write_component_install_manifest \
  "${WORK}/${BASENAME}/bin" \
  "${PACKAGE_ID}" \
  "${VERSION}" \
  "${TARGET}" \
  "${WORK}/${BASENAME}/install-manifest.json" \
  "${BUILT_AT}"

tar -czf "${ARCHIVE_PATH}" -C "${WORK}" "${BASENAME}"
rm -rf "${WORK}"

HASH="$(medousa_sha256_file "${ARCHIVE_PATH}")"
medousa_log "sha256: ${HASH}"

if [[ -f "${CHECKSUMS_FILE}" ]]; then
  grep -v "  ${ARCHIVE_NAME}$" "${CHECKSUMS_FILE}" >"${CHECKSUMS_FILE}.tmp" || true
  mv "${CHECKSUMS_FILE}.tmp" "${CHECKSUMS_FILE}"
fi
echo "${HASH}  ${ARCHIVE_NAME}" >>"${CHECKSUMS_FILE}"

medousa_log "wrote ${ARCHIVE_PATH}"
