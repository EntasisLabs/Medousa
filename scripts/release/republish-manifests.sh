#!/usr/bin/env bash
# Regenerate release-manifest.json + installer-bootstrap.json from artifacts already
# on R2 (or a local staging dir) and upload only those JSON files — no rebuild.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

STAGING_DIR=""
VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
BASE_URL_OVERRIDE=""
FROM_R2=0
UPLOAD=0
SKIP_DOWNLOAD=0

usage() {
  cat <<'EOF'
Usage: scripts/release/republish-manifests.sh [options]

Rebuild release-manifest.json and installer-bootstrap.json from existing release
artifacts without recompiling. Optionally sync artifacts down from R2 and/or
upload only the regenerated JSON files back.

Options:
  --from-r2             Download release artifacts from R2 into staging first
  --staging <dir>       Artifact directory (default: dist/republish-staging)
  --version <ver>       Release version without v prefix (default: Cargo.toml)
  --channel <name>      Release channel (default: stable)
  --base-url <url>      Override MEDOUSA_RELEASE_BASE_URL
  --skip-download       Use existing staging dir (with --from-r2)
  --upload              Upload only release-manifest.json + installer-bootstrap.json
  -h, --help            Show this help

Required for --from-r2 / --upload:
  MEDOUSA_R2_BUCKET, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
  MEDOUSA_R2_ENDPOINT (or AWS_ENDPOINT_URL)
  MEDOUSA_R2_PREFIX (default: medousa/<channel>)

Typical CI recovery (after fixing registry finders / bootstrap paths):
  export MEDOUSA_RELEASE_BASE_URL=https://releases.entasislabs.com/medousa
  ./scripts/release/republish-manifests.sh --from-r2 --upload

Local recovery when you already have dist/final from a prior publish:
  ./scripts/release/republish-manifests.sh --staging dist/final --upload
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-r2) FROM_R2=1; shift ;;
    --staging) STAGING_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --base-url) BASE_URL_OVERRIDE="$2"; shift 2 ;;
    --skip-download) SKIP_DOWNLOAD=1; shift ;;
    --upload) UPLOAD=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

STAGING_DIR="${STAGING_DIR:-${ROOT}/dist/republish-staging}"
if [[ "${STAGING_DIR}" != /* ]]; then
  STAGING_DIR="${ROOT}/${STAGING_DIR}"
fi
mkdir -p "${STAGING_DIR}"
STAGING_DIR="$(cd "${STAGING_DIR}" && pwd)"

VERSION="${VERSION:-$(medousa_max_package_version)}"
PREFIX="${MEDOUSA_R2_PREFIX:-medousa/${CHANNEL}}"
ENDPOINT="${MEDOUSA_R2_ENDPOINT:-${AWS_ENDPOINT_URL:-}}"
BUCKET="${MEDOUSA_R2_BUCKET:-}"

if [[ "${FROM_R2}" -eq 1 && "${SKIP_DOWNLOAD}" -eq 0 ]]; then
  if [[ -z "${BUCKET}" ]]; then
    echo "error: set MEDOUSA_R2_BUCKET for --from-r2" >&2
    exit 1
  fi
  if [[ -z "${AWS_ACCESS_KEY_ID:-}" || -z "${AWS_SECRET_ACCESS_KEY:-}" ]]; then
    echo "error: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY for --from-r2" >&2
    exit 1
  fi
  medousa_require_cmd aws

  S3_URI="s3://${BUCKET}/${PREFIX%/}/"
  # Only pull current package stamps — a full channel history dump made the
  # old "first Medousa_* wins" finders latch onto 0.1.0 forever.
  DESKTOP_V="$(medousa_package_version desktop)"
  INSTALLER_V="$(medousa_package_version installer)"
  BRAIN_V="$(medousa_package_version local-brain)"
  AWS_ARGS=(s3 sync "${S3_URI}" "${STAGING_DIR}/"
    --exclude "*"
    --include "SHA256SUMS"
    --include "Medousa_${DESKTOP_V}_*"
    --include "Medousa Installer_${INSTALLER_V}_*"
    --include "MedousaInstaller_${INSTALLER_V}_*"
    --include "medousa_local-*-v${BRAIN_V}-*"
    --include "model-*-v${BRAIN_V}.manifest.json")
  pkg_id=""
  pkg_ver=""
  for pkg_id in "${MEDOUSA_PACKAGE_VERSION_IDS[@]}"; do
    [[ "${pkg_id}" == desktop || "${pkg_id}" == local-brain ]] && continue
    pkg_ver="$(medousa_package_version "${pkg_id}")"
    AWS_ARGS+=(--include "${pkg_id}-v${pkg_ver}-*")
  done
  if [[ -n "${ENDPOINT}" ]]; then
    AWS_ARGS+=(--endpoint-url "${ENDPOINT}")
  fi

  medousa_log "downloading current-stamp artifacts from ${S3_URI} (desktop=${DESKTOP_V} installer=${INSTALLER_V})"
  aws "${AWS_ARGS[@]}"
fi

if ! compgen -G "${STAGING_DIR}/*" >/dev/null; then
  echo "error: no artifacts in ${STAGING_DIR} — use --from-r2 or --staging" >&2
  exit 1
fi

GEN_ARGS=(--dist "${STAGING_DIR}" --version "${VERSION}" --channel "${CHANNEL}")
[[ -n "${BASE_URL_OVERRIDE}" ]] && GEN_ARGS+=(--base-url "${BASE_URL_OVERRIDE}")

medousa_log "regenerating manifests for v${VERSION} channel=${CHANNEL} from ${STAGING_DIR}"
"${SCRIPT_DIR}/generate-release-manifest.sh" "${GEN_ARGS[@]}"
"${SCRIPT_DIR}/generate-installer-bootstrap.sh" "${GEN_ARGS[@]}"

medousa_assert_release_manifest_nonempty "${STAGING_DIR}/release-manifest.json"
medousa_assert_installer_bootstrap_nonempty "${STAGING_DIR}/installer-bootstrap.json"

medousa_log "manifest packages: $(jq '.packages | length' "${STAGING_DIR}/release-manifest.json")"
medousa_log "bootstrap platforms: $(jq '.platforms | length' "${STAGING_DIR}/installer-bootstrap.json")"

if [[ "${UPLOAD}" -eq 1 ]]; then
  if [[ -z "${BUCKET}" ]]; then
    echo "error: set MEDOUSA_R2_BUCKET for --upload" >&2
    exit 1
  fi
  if [[ -z "${AWS_ACCESS_KEY_ID:-}" || -z "${AWS_SECRET_ACCESS_KEY:-}" ]]; then
    echo "error: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY for --upload" >&2
    exit 1
  fi
  medousa_require_cmd aws

  S3_URI="s3://${BUCKET}/${PREFIX%/}/"
  for file in release-manifest.json installer-bootstrap.json; do
    AWS_ARGS=(s3 cp "${STAGING_DIR}/${file}" "${S3_URI}${file}"
      --cache-control "public, max-age=300"
      --content-type "application/json")
    if [[ -n "${ENDPOINT}" ]]; then
      AWS_ARGS+=(--endpoint-url "${ENDPOINT}")
    fi
    medousa_log "upload ${file} → ${S3_URI}${file}"
    aws "${AWS_ARGS[@]}"
  done
fi

if [[ -n "${MEDOUSA_RELEASE_BASE_URL:-${BASE_URL_OVERRIDE}}" ]]; then
  base="${MEDOUSA_RELEASE_BASE_URL:-${BASE_URL_OVERRIDE}}"
  medousa_log "public manifest: ${base%/}/${CHANNEL}/release-manifest.json"
  medousa_log "public bootstrap: ${base%/}/${CHANNEL}/installer-bootstrap.json"
fi

medousa_log "republish complete"
