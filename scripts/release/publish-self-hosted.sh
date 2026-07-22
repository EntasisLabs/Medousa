#!/usr/bin/env bash
# Self-hosted release publish contract — stage artifacts, generate manifests, ready for upload.
#
# Your CI/CD pipeline should:
#   1. Build per-arch (build.sh + package-all-components.sh + desktop/installer bundles)
#   2. Collect artifacts into a staging directory
#   3. Run this script with MEDOUSA_RELEASE_BASE_URL set
#   4. Upload dist/final/* to your artifact registry
#   5. Promote latest/ pointers after smoke verify

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

STAGING_DIR=""
VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
BASE_URL_OVERRIDE=""
MERGE_BASE_DIR=""
FULL_TRAIN=0

usage() {
  cat <<'EOF'
Usage: scripts/release/publish-self-hosted.sh [options]

Options:
  --staging <dir>       Directory with built artifacts (default: dist/)
  --version <ver>       Channel-head version without v prefix
  --channel <name>      Release channel (default: stable)
  --base-url <url>      Artifact base URL (or set MEDOUSA_RELEASE_BASE_URL)
  --merge-base <dir>    Dir containing existing channel release-manifest.json
                        + installer-bootstrap.json to merge (targeted releases)
  --full-train          Do not merge — replace channel indexes wholesale
  -h, --help            Show this help

Writes to dist/final/:
  - release-manifest.json
  - installer-bootstrap.json
  - SHA256SUMS (copied from staging if present)
  - copies of all release artifacts

Set MEDOUSA_RELEASE_BASE_URL before running in CI, e.g.:
  export MEDOUSA_RELEASE_BASE_URL=https://releases.example.com/medousa
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --staging) STAGING_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --base-url) BASE_URL_OVERRIDE="$2"; shift 2 ;;
    --merge-base) MERGE_BASE_DIR="$2"; shift 2 ;;
    --full-train) FULL_TRAIN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

STAGING_DIR="${STAGING_DIR:-${ROOT}/dist}"
if [[ "${STAGING_DIR}" != /* ]]; then
  STAGING_DIR="${ROOT}/${STAGING_DIR}"
fi
STAGING_DIR="$(cd "${STAGING_DIR}" && pwd)"

VERSION="${VERSION:-$(medousa_max_package_version)}"
FINAL_DIR="$(mkdir -p "${ROOT}/dist/final" && cd "${ROOT}/dist/final" && pwd)"

medousa_log "staging release channel-head v${VERSION} channel=${CHANNEL}"

if [[ "${STAGING_DIR}" == "${FINAL_DIR}" ]]; then
  medousa_log "staging dir is final output (${FINAL_DIR}) — skipping artifact copy"
else
  # Copy all release artifacts into final/
  shopt -s nullglob
  for pattern in \
    "*.tar.gz" \
    "*.dmg" "*.msi" "*.exe" \
    "*.AppImage" "*.deb" \
    "model-*.manifest.json" \
    "SHA256SUMS"; do
    for file in "${STAGING_DIR}"/${pattern}; do
      cp -a "${file}" "${FINAL_DIR}/"
    done
  done
  shopt -u nullglob

  # Also pull desktop/installer bundles from subdirs
  find "${STAGING_DIR}" -maxdepth 3 -type f \( \
    -name 'Medousa_*.dmg' -o \
    -name 'Medousa_*.msi' -o \
    -name 'Medousa_*.exe' -o \
    -name 'Medousa_*.AppImage' -o \
    -name 'Medousa_*.deb' -o \
    -name 'MedousaInstaller*.dmg' -o \
    -name 'MedousaInstaller*.msi' -o \
    -name 'MedousaInstaller*.exe' -o \
    -name 'MedousaInstaller*.AppImage' -o \
    -name 'MedousaInstaller*.deb' -o \
    -name 'Medousa Installer*.dmg' -o \
    -name 'Medousa Installer*.msi' -o \
    -name 'Medousa Installer*.exe' -o \
    -name 'Medousa Installer*.AppImage' -o \
    -name 'Medousa Installer*.deb' \
    \) -exec cp -a {} "${FINAL_DIR}/" \;
fi

GEN_ARGS=(--dist "${FINAL_DIR}" --version "${VERSION}" --channel "${CHANNEL}")
[[ -n "${BASE_URL_OVERRIDE}" ]] && GEN_ARGS+=(--base-url "${BASE_URL_OVERRIDE}")

MEDOUSA_RELEASE_CHANNEL="${CHANNEL}"
[[ -n "${BASE_URL_OVERRIDE}" ]] && MEDOUSA_RELEASE_BASE_URL="${BASE_URL_OVERRIDE}"

DELTA_MANIFEST="${FINAL_DIR}/release-manifest.delta.json"
DELTA_BOOTSTRAP="${FINAL_DIR}/installer-bootstrap.delta.json"

"${SCRIPT_DIR}/generate-release-manifest.sh" "${GEN_ARGS[@]}"
mv -f "${FINAL_DIR}/release-manifest.json" "${DELTA_MANIFEST}"
"${SCRIPT_DIR}/generate-installer-bootstrap.sh" "${GEN_ARGS[@]}"
# Bootstrap generator may no-op / fail assert when no desktop/installer in staging.
if [[ -f "${FINAL_DIR}/installer-bootstrap.json" ]]; then
  mv -f "${FINAL_DIR}/installer-bootstrap.json" "${DELTA_BOOTSTRAP}"
fi

BASE_MANIFEST=""
BASE_BOOTSTRAP=""
if [[ "${FULL_TRAIN}" -eq 0 && -n "${MERGE_BASE_DIR}" ]]; then
  [[ -f "${MERGE_BASE_DIR}/release-manifest.json" ]] && BASE_MANIFEST="${MERGE_BASE_DIR}/release-manifest.json"
  [[ -f "${MERGE_BASE_DIR}/installer-bootstrap.json" ]] && BASE_BOOTSTRAP="${MERGE_BASE_DIR}/installer-bootstrap.json"
fi

if [[ "${FULL_TRAIN}" -eq 1 || -z "${BASE_MANIFEST}" ]]; then
  mv -f "${DELTA_MANIFEST}" "${FINAL_DIR}/release-manifest.json"
else
  "${SCRIPT_DIR}/merge-release-manifest.sh" \
    --base "${BASE_MANIFEST}" \
    --delta "${DELTA_MANIFEST}" \
    --out "${FINAL_DIR}/release-manifest.json" \
    --channel-head "${VERSION}"
  rm -f "${DELTA_MANIFEST}"
fi

if [[ -f "${DELTA_BOOTSTRAP}" ]]; then
  if [[ "${FULL_TRAIN}" -eq 1 || -z "${BASE_BOOTSTRAP}" ]]; then
    mv -f "${DELTA_BOOTSTRAP}" "${FINAL_DIR}/installer-bootstrap.json"
  else
    "${SCRIPT_DIR}/merge-installer-bootstrap.sh" \
      --base "${BASE_BOOTSTRAP}" \
      --delta "${DELTA_BOOTSTRAP}" \
      --out "${FINAL_DIR}/installer-bootstrap.json" \
      --channel-head "${VERSION}"
    rm -f "${DELTA_BOOTSTRAP}"
  fi
elif [[ -n "${BASE_BOOTSTRAP}" && "${FULL_TRAIN}" -eq 0 ]]; then
  # Targeted ship without desktop/installer — keep prior bootstrap.
  cp -f "${BASE_BOOTSTRAP}" "${FINAL_DIR}/installer-bootstrap.json"
fi

medousa_assert_release_manifest_nonempty "${FINAL_DIR}/release-manifest.json"
if [[ -f "${FINAL_DIR}/installer-bootstrap.json" ]]; then
  medousa_assert_installer_bootstrap_nonempty "${FINAL_DIR}/installer-bootstrap.json"
fi

medousa_log "publish-ready artifacts in ${FINAL_DIR}"
medousa_log "manifest: $(medousa_release_manifest_url)"
medousa_log "bootstrap: $(medousa_release_bootstrap_url)"
ls -la "${FINAL_DIR}"
