#!/usr/bin/env bash
# Generate installer-bootstrap.json — per-platform Medousa Installer download URLs.

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
  --dist <dir>          Directory containing installer bundles (default: dist/)
  --version <version>   Release version without v prefix
  --channel <name>      Release channel (default: stable)
  --base-url <url>      Override MEDOUSA_RELEASE_BASE_URL
  -h, --help            Show this help

Writes dist/installer-bootstrap.json with per-platform installer download URLs.
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
VERSION="${VERSION:-$(medousa_version)}"

if [[ -n "${BASE_URL_OVERRIDE}" ]]; then
  BASE_URL="${BASE_URL_OVERRIDE%/}/${CHANNEL}"
else
  MEDOUSA_RELEASE_CHANNEL="${CHANNEL}"
  BASE_URL="$(medousa_release_base_url "${VERSION}")"
fi

find_installer() {
  local pattern="$1"
  find "${DIST_DIR}" -maxdepth 3 -type f -name "${pattern}" 2>/dev/null | head -1
}

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

append_platform_json() {
  local platform="$1"
  local path="$2"
  local url sha size
  url="$(url_for_file "${path}")"
  sha="$(sha_for "${path}")"
  size="$(size_for "${path}")"
  local name
  name="$(basename "${path:-}")"
  cat <<EOF
    "${platform}": {
      "platform": "${platform}",
      "version": "${VERSION}",
      "fileName": "${name}",
      "url": "${url}",
      "sha256": "${sha}",
      "sizeBytes": ${size}
    }
EOF
}

MAC_DMG="$(find_installer 'MedousaInstaller*.dmg')"
WIN_MSI="$(find_installer 'MedousaInstaller*.msi')"
WIN_EXE="$(find_installer 'MedousaInstaller*.exe')"
LINUX_APPIMAGE="$(find_installer 'MedousaInstaller*.AppImage')"
LINUX_DEB="$(find_installer 'MedousaInstaller*.deb')"

OUT="${DIST_DIR}/installer-bootstrap.json"
PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

{
  echo "{"
  echo '  "schemaVersion": 1,'
  echo '  "product": "medousa-installer",'
  echo "  \"version\": \"${VERSION}\","
  echo "  \"channel\": \"${CHANNEL}\","
  echo "  \"publishedAt\": \"${PUBLISHED_AT}\","
  echo "  \"manifestUrl\": \"${BASE_URL}/release-manifest.json\","
  echo '  "platforms": {'

  first=1
  for entry in \
    "macos-aarch64:${MAC_DMG}" \
    "macos-x64:${MAC_DMG}" \
    "windows-x64:${WIN_MSI:-$WIN_EXE}" \
    "linux-x64:${LINUX_APPIMAGE:-$LINUX_DEB}"; do
    platform="${entry%%:*}"
    path="${entry#*:}"
    [[ -n "${path}" ]] || continue
    [[ "${first}" -eq 1 ]] || echo ","
    first=0
    append_platform_json "${platform}" "${path}"
  done

  echo ""
  echo "  }"
  echo "}"
} >"${OUT}"

medousa_log "wrote ${OUT}"
