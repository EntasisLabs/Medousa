#!/usr/bin/env bash
# Package medousa_local into a release tarball.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
INPUT=""
DIST_DIR=""
BACKEND="auto"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --input) INPUT="$2"; shift 2 ;;
    --dist) DIST_DIR="$2"; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
cd "${ROOT}"
VERSION="$(medousa_version)"
TARGET="${TARGET:-$(medousa_host_target)}"
INPUT="${INPUT:-${ROOT}/dist/build-local-brain/${TARGET}}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"

BIN_DIR="${INPUT}/bin"
if [[ ! -d "${BIN_DIR}" ]]; then
  echo "error: missing ${BIN_DIR}" >&2
  exit 1
fi

ARCHIVE_NAME="medousa_local-${BACKEND}-v${VERSION}-${TARGET}.tar.gz"
ARCHIVE_PATH="${DIST_DIR}/${ARCHIVE_NAME}"
BASENAME="medousa_local-${BACKEND}-v${VERSION}-${TARGET}"
WORK="${DIST_DIR}/.pack-work-${BASENAME}"
rm -rf "${WORK}"
mkdir -p "${WORK}/${BASENAME}/bin"
cp -a "${BIN_DIR}/." "${WORK}/${BASENAME}/bin/"

tar -czf "${ARCHIVE_PATH}" -C "${WORK}" "${BASENAME}"
rm -rf "${WORK}"

HASH="$(medousa_sha256_file "${ARCHIVE_PATH}")"
CHECKSUMS_FILE="${DIST_DIR}/SHA256SUMS"
if [[ -f "${CHECKSUMS_FILE}" ]]; then
  grep -v "  ${ARCHIVE_NAME}$" "${CHECKSUMS_FILE}" >"${CHECKSUMS_FILE}.tmp" || true
  mv "${CHECKSUMS_FILE}.tmp" "${CHECKSUMS_FILE}"
fi
echo "${HASH}  ${ARCHIVE_NAME}" >>"${CHECKSUMS_FILE}"
medousa_log "wrote ${ARCHIVE_PATH}"
