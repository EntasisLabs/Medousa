#!/usr/bin/env bash
# Stage local builds, generate manifests, optionally upload to R2.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

STAGING_DIR=""
VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
UPLOAD=0
WRITE_INSTALLER_CONFIG=1

usage() {
  cat <<'EOF'
Usage: scripts/release/publish-local.sh [options]

One-shot publish for local/self-hosted releases:
  1. (optional) write installer-config.json from MEDOUSA_RELEASE_BASE_URL
  2. generate release-manifest.json + installer-bootstrap.json in dist/final
  3. (optional) upload dist/final to Cloudflare R2

Options:
  --staging <dir>     Artifact directory (default: dist/)
  --version <ver>     Version without v prefix (default: Cargo.toml)
  --channel <name>    Release channel (default: stable)
  --base-url <url>    Override MEDOUSA_RELEASE_BASE_URL
  --upload            Run upload-r2.sh after publish
  --skip-config       Do not rewrite installer-config.json
  -h, --help          Show this help

Required for production:
  export MEDOUSA_RELEASE_BASE_URL=https://releases.medousa.app/medousa

Typical flow:
  # 1. Build artifacts on each platform, copy into dist/
  # 2. Sign Windows/Mac binaries
  # 3. Publish + upload:
  export MEDOUSA_RELEASE_BASE_URL=https://releases.medousa.app/medousa
  ./scripts/release/publish-local.sh --upload
  # 4. Rebuild installer on each platform AFTER set-installer-config (or use --skip-config if already built)
  ./scripts/release/set-installer-config.sh
  cd apps/medousa-installer && npm run tauri build
EOF
}

BASE_URL_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --staging) STAGING_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --base-url) BASE_URL_OVERRIDE="$2"; shift 2 ;;
    --upload) UPLOAD=1; shift ;;
    --skip-config) WRITE_INSTALLER_CONFIG=0; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
STAGING_DIR="${STAGING_DIR:-${ROOT}/dist}"
VERSION="${VERSION:-$(medousa_version)}"

if [[ -n "${BASE_URL_OVERRIDE}" ]]; then
  export MEDOUSA_RELEASE_BASE_URL="${BASE_URL_OVERRIDE}"
fi

if [[ -z "${MEDOUSA_RELEASE_BASE_URL:-}" ]]; then
  echo "error: set MEDOUSA_RELEASE_BASE_URL (your R2 public URL base, no trailing slash)" >&2
  echo "  example: export MEDOUSA_RELEASE_BASE_URL=https://releases.medousa.app/medousa" >&2
  exit 1
fi

export MEDOUSA_RELEASE_CHANNEL="${CHANNEL}"

if [[ "${WRITE_INSTALLER_CONFIG}" -eq 1 ]]; then
  "${SCRIPT_DIR}/set-installer-config.sh"
fi

PUBLISH_ARGS=(--staging "${STAGING_DIR}" --version "${VERSION}" --channel "${CHANNEL}" --base-url "${MEDOUSA_RELEASE_BASE_URL}")
"${SCRIPT_DIR}/publish-self-hosted.sh" "${PUBLISH_ARGS[@]}"

medousa_log "publish complete — dist/final ready"
medousa_log "manifest: $(MEDOUSA_RELEASE_BASE_URL="${MEDOUSA_RELEASE_BASE_URL}" MEDOUSA_RELEASE_CHANNEL="${CHANNEL}" medousa_release_manifest_url)"
medousa_log "bootstrap: $(MEDOUSA_RELEASE_BASE_URL="${MEDOUSA_RELEASE_BASE_URL}" MEDOUSA_RELEASE_CHANNEL="${CHANNEL}" medousa_release_bootstrap_url)"

if [[ "${UPLOAD}" -eq 1 ]]; then
  "${SCRIPT_DIR}/upload-r2.sh" "${ROOT}/dist/final"
fi
