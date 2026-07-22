#!/usr/bin/env bash
# Package a build staging directory into a release tarball and SHA256SUMS entry.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
INPUT=""
DIST_DIR=""

usage() {
  cat <<'EOF'
Usage: scripts/release/package.sh [options]

Options:
  --target <triple>   Rust target triple (default: host, or from build-meta.env)
  --input <dir>       Staging dir from build.sh (contains bin/ and build-meta.env)
  --dist <dir>        Output directory for archives (default: dist/)
  -h, --help          Show this help

Creates dist/medousa-vX.Y.Z-<target>.tar.gz and appends to dist/SHA256SUMS.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --input)
      INPUT="$2"
      shift 2
      ;;
    --dist)
      DIST_DIR="$2"
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

medousa_require_cmd tar

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

# Full-suite archive tracks the engine package stamp.
VERSION="$(medousa_package_version engine)"

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

ARCHIVE_NAME="$(medousa_asset_archive_name "${VERSION}" "${TARGET}")"
ARCHIVE_PATH="${DIST_DIR}/${ARCHIVE_NAME}"
BASENAME="$(medousa_asset_basename "${VERSION}" "${TARGET}")"
CHECKSUMS_FILE="${DIST_DIR}/SHA256SUMS"

medousa_log "packaging ${ARCHIVE_NAME}"

# Archive layout: medousa-vX.Y.Z-target/bin/<binaries>
WORK="${DIST_DIR}/.pack-work-${BASENAME}"
rm -rf "${WORK}"
mkdir -p "${WORK}/${BASENAME}/bin"
cp -a "${BIN_DIR}/." "${WORK}/${BASENAME}/bin/"

BUILT_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
medousa_write_install_manifest \
  "${WORK}/${BASENAME}/bin" \
  "${VERSION}" \
  "${TARGET}" \
  "${WORK}/${BASENAME}/install-manifest.json" \
  "${BUILT_AT}"
medousa_log "install-manifest component_set_id=$(medousa_read_manifest_field "${WORK}/${BASENAME}/install-manifest.json" component_set_id)"

tar -czf "${ARCHIVE_PATH}" -C "${WORK}" "${BASENAME}"
rm -rf "${WORK}"

HASH="$(medousa_sha256_file "${ARCHIVE_PATH}")"
medousa_log "sha256: ${HASH}"

# Replace existing line for this archive if re-packaging
if [[ -f "${CHECKSUMS_FILE}" ]]; then
  grep -v "  ${ARCHIVE_NAME}$" "${CHECKSUMS_FILE}" >"${CHECKSUMS_FILE}.tmp" || true
  mv "${CHECKSUMS_FILE}.tmp" "${CHECKSUMS_FILE}"
fi
echo "${HASH}  ${ARCHIVE_NAME}" >>"${CHECKSUMS_FILE}"

medousa_log "wrote ${ARCHIVE_PATH}"
medousa_log "updated ${CHECKSUMS_FILE}"
