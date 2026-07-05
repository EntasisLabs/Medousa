#!/usr/bin/env bash
# Upload dist/final (or custom dir) to Cloudflare R2 (S3-compatible).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

SOURCE_DIR=""
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: scripts/release/upload-r2.sh [options] [source-dir]

Upload release artifacts to Cloudflare R2.

Options:
  --source <dir>   Directory to upload (default: dist/final)
  --dry-run        Print aws s3 sync command without running
  -h, --help       Show this help

Required environment:
  MEDOUSA_R2_BUCKET          R2 bucket name
  AWS_ACCESS_KEY_ID          R2 access key id
  AWS_SECRET_ACCESS_KEY      R2 secret access key

Optional environment:
  MEDOUSA_R2_ENDPOINT        e.g. https://<accountid>.r2.cloudflarestorage.com
  AWS_ENDPOINT_URL           alias for MEDOUSA_R2_ENDPOINT
  MEDOUSA_RELEASE_BASE_URL     used to derive object prefix when MEDOUSA_R2_PREFIX unset
  MEDOUSA_R2_PREFIX          object key prefix (default: medousa/stable)
  MEDOUSA_RELEASE_CHANNEL    channel segment (default: stable)

Example:
  export MEDOUSA_RELEASE_BASE_URL=https://releases.medousa.app/medousa
  export MEDOUSA_R2_BUCKET=medousa-releases
  export MEDOUSA_R2_ENDPOINT=https://abc123.r2.cloudflarestorage.com
  export AWS_ACCESS_KEY_ID=...
  export AWS_SECRET_ACCESS_KEY=...
  ./scripts/release/upload-r2.sh
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source) SOURCE_DIR="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    -*) echo "error: unknown option: $1" >&2; exit 1 ;;
    *)
      if [[ -z "${SOURCE_DIR}" ]]; then
        SOURCE_DIR="$1"
        shift
      else
        echo "error: unexpected argument: $1" >&2
        exit 1
      fi
      ;;
  esac
done

ROOT="$(medousa_repo_root)"
SOURCE_DIR="${SOURCE_DIR:-${ROOT}/dist/final}"
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
PREFIX="${MEDOUSA_R2_PREFIX:-medousa/${CHANNEL}}"
ENDPOINT="${MEDOUSA_R2_ENDPOINT:-${AWS_ENDPOINT_URL:-}}"
BUCKET="${MEDOUSA_R2_BUCKET:-}"

if [[ ! -d "${SOURCE_DIR}" ]]; then
  echo "error: source directory not found: ${SOURCE_DIR}" >&2
  exit 1
fi

if [[ -z "${BUCKET}" ]]; then
  echo "error: set MEDOUSA_R2_BUCKET" >&2
  exit 1
fi

if [[ -z "${AWS_ACCESS_KEY_ID:-}" || -z "${AWS_SECRET_ACCESS_KEY:-}" ]]; then
  echo "error: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY (R2 API token)" >&2
  exit 1
fi

if ! command -v aws >/dev/null 2>&1; then
  echo "error: aws CLI not found — install AWS CLI v2" >&2
  exit 1
fi

if [[ ! -f "${SOURCE_DIR}/release-manifest.json" ]]; then
  echo "warning: ${SOURCE_DIR}/release-manifest.json missing — run publish-self-hosted.sh first" >&2
fi

S3_URI="s3://${BUCKET}/${PREFIX%/}/"
AWS_ARGS=(s3 sync "${SOURCE_DIR}/" "${S3_URI}"
  --cache-control "public, max-age=300"
  --exclude ".*")

if [[ -n "${ENDPOINT}" ]]; then
  AWS_ARGS+=(--endpoint-url "${ENDPOINT}")
fi

medousa_log "upload ${SOURCE_DIR} → ${S3_URI}"
if [[ -n "${MEDOUSA_RELEASE_BASE_URL:-}" ]]; then
  medousa_log "public manifest: ${MEDOUSA_RELEASE_BASE_URL%/}/${CHANNEL}/release-manifest.json"
fi

if [[ "${DRY_RUN}" -eq 1 ]]; then
  printf 'aws'
  printf ' %q' "${AWS_ARGS[@]}"
  echo
  exit 0
fi

aws "${AWS_ARGS[@]}"
medousa_log "upload complete"
