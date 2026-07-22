#!/usr/bin/env bash
# Generate installer-bootstrap.json — per-platform default download URLs for medousa.app.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

DIST_DIR=""
VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"

usage() {
  cat <<'EOF'
Usage: scripts/release/generate-installer-bootstrap.sh [options]

Options:
  --dist <dir>          Directory containing release bundles (default: dist/)
  --version <version>   Release version without v prefix
  --channel <name>      Release channel (default: stable)
  --base-url <url>      Override MEDOUSA_RELEASE_BASE_URL
  -h, --help            Show this help

Writes dist/installer-bootstrap.json with per-platform download URLs.
Windows points at the signed desktop NSIS setup; Mac/Linux use Medousa Installer.
EOF
}

BASE_URL_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dist) DIST_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --base-url) BASE_URL_OVERRIDE="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
cd "${ROOT}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"
VERSION="${VERSION:-$(medousa_max_package_version)}"

if [[ -n "${BASE_URL_OVERRIDE}" ]]; then
  BASE_URL="${BASE_URL_OVERRIDE%/}/${CHANNEL}"
else
  MEDOUSA_RELEASE_CHANNEL="${CHANNEL}"
  BASE_URL="$(medousa_release_base_url "${VERSION}")"
fi

url_for_file() {
  local path="$1"
  if [[ -z "${path}" ]]; then
    echo ""
    return 0
  fi
  echo "${BASE_URL}/$(basename "${path}")"
}

sha_for() {
  local file="$1"
  local name
  name="$(basename "${file}")"
  if [[ -f "${DIST_DIR}/SHA256SUMS" ]]; then
    awk -v f="${name}" '$2 == f {print $1; exit}' "${DIST_DIR}/SHA256SUMS"
  elif [[ -f "${file}" ]]; then
    medousa_sha256_file "${file}"
  fi
}

size_for() {
  local path="$1"
  if [[ -f "${path}" ]]; then
    stat -f%z "${path}" 2>/dev/null || stat -c%s "${path}"
  else
    echo 0
  fi
}

artifact_kind_for_platform() {
  local platform="$1"
  case "${platform}" in
    windows-x64) echo "desktop" ;;
    *) echo "installer" ;;
  esac
}

append_platform_json() {
  local platform="$1"
  local path="$2"
  local installer_path="${3:-}"
  local url sha size kind installer_url
  url="$(url_for_file "${path}")"
  sha="$(sha_for "${path}")"
  size="$(size_for "${path}")"
  kind="$(artifact_kind_for_platform "${platform}")"
  installer_url="$(url_for_file "${installer_path}")"
  local name
  name="$(basename "${path:-}")"
  cat <<EOF
    "${platform}": {
      "platform": "${platform}",
      "artifactKind": "${kind}",
      "version": "${VERSION}",
      "fileName": "${name}",
      "url": "${url}",
      "sha256": "${sha}",
      "sizeBytes": ${size}
EOF
  if [[ -n "${installer_url}" ]]; then
    cat <<EOF
,
      "installerUrl": "${installer_url}"
EOF
  fi
  echo "    }"
}

OUT="${DIST_DIR}/installer-bootstrap.json"
PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

WIN_INSTALLER="$(medousa_installer_bundle_for_platform "${DIST_DIR}" windows-x64 || true)"

{
  echo "{"
  echo '  "schemaVersion": 1,'
  echo '  "product": "medousa",'
  echo "  \"version\": \"${VERSION}\","
  echo "  \"channel\": \"${CHANNEL}\","
  echo "  \"publishedAt\": \"${PUBLISHED_AT}\","
  echo "  \"manifestUrl\": \"${BASE_URL}/release-manifest.json\","
  echo '  "platforms": {'

  first=1
  for platform in macos-aarch64 macos-x64 windows-x64 linux-x64; do
    path="$(medousa_bootstrap_bundle_for_platform "${DIST_DIR}" "${platform}" || true)"
    [[ -n "${path}" ]] || continue
    installer_extra=""
    if [[ "${platform}" == "windows-x64" && -n "${WIN_INSTALLER}" ]]; then
      installer_extra="${WIN_INSTALLER}"
    fi
    [[ "${first}" -eq 1 ]] || echo ","
    first=0
    append_platform_json "${platform}" "${path}" "${installer_extra}"
  done

  echo ""
  echo "  }"
  echo "}"
} >"${OUT}"

# Targeted releases may ship no desktop/installer — leave an empty platforms map
# so merge-installer-bootstrap can keep the prior channel bootstrap.
if grep -q '"platforms": {}' "${OUT}" 2>/dev/null || grep -q '"platforms": {\n\n  }' "${OUT}" 2>/dev/null; then
  medousa_log "wrote ${OUT} (no platforms in staging — ok for component-only ships)"
elif ! grep -q '"platform":' "${OUT}"; then
  medousa_log "wrote ${OUT} (no platforms in staging — ok for component-only ships)"
else
  medousa_assert_installer_bootstrap_nonempty "${OUT}"
  medousa_log "wrote ${OUT}"
fi
