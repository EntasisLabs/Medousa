#!/usr/bin/env bash
# Medousa desktop app — download the right installer for this machine and open it.
#
# Self-contained (safe under `curl | bash`). Resolves the artifact from
# installer-bootstrap.json on the release CDN.
#
#   curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install-app.sh | bash
#
# Options (after bash -s --):
#   --channel <name>       Release channel (default: stable)
#   --bootstrap-url <url>  Override bootstrap JSON URL
#   --download-only        Download but do not open/run the installer
#   --dir <path>           Download directory (default: ~/Downloads or /tmp)
#   -h, --help

set -euo pipefail

BOOTSTRAP_BASE="${MEDOUSA_RELEASE_BASE_URL:-https://releases.entasislabs.com/medousa}"
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
BOOTSTRAP_URL="${MEDOUSA_INSTALLER_BOOTSTRAP_URL:-}"
DOWNLOAD_ONLY=0
OUT_DIR=""

usage() {
  cat <<'EOF'
Usage: install-app.sh [options]

Download the Medousa desktop app for this OS/arch and start the installer.

  curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install-app.sh | bash

Options:
  --channel <name>       Release channel (default: stable)
  --bootstrap-url <url>  Full URL to installer-bootstrap.json
  --download-only        Save the file; do not open/run it
  --dir <path>           Where to save the download
  -h, --help             Show this help

Environment:
  MEDOUSA_RELEASE_BASE_URL           CDN base (default: https://releases.entasislabs.com/medousa)
  MEDOUSA_RELEASE_CHANNEL            Channel (default: stable)
  MEDOUSA_INSTALLER_BOOTSTRAP_URL    Override bootstrap JSON URL
EOF
}

log() { echo "[medousa-app] $*"; }
die() { echo "[medousa-app] error: $*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)
      CHANNEL="${2:-}"
      [[ -n "${CHANNEL}" ]] || die "--channel requires a value"
      shift 2
      ;;
    --bootstrap-url)
      BOOTSTRAP_URL="${2:-}"
      [[ -n "${BOOTSTRAP_URL}" ]] || die "--bootstrap-url requires a value"
      shift 2
      ;;
    --download-only)
      DOWNLOAD_ONLY=1
      shift
      ;;
    --dir)
      OUT_DIR="${2:-}"
      [[ -n "${OUT_DIR}" ]] || die "--dir requires a value"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1 (try --help)"
      ;;
  esac
done

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

require_cmd curl

bootstrap_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Darwin:arm64|Darwin:aarch64) echo "macos-aarch64" ;;
    Darwin:x86_64) echo "macos-x64" ;;
    Linux:x86_64) echo "linux-x64" ;;
    Linux:aarch64|Linux:arm64)
      die "Linux arm64 desktop builds are not published yet — use an x86_64 machine or the CLI installer (scripts/install.sh)"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      case "${arch}" in
        x86_64|amd64) echo "windows-x64" ;;
        *) die "unsupported Windows arch: ${arch}" ;;
      esac
      ;;
    *)
      die "unsupported platform ${os}/${arch}"
      ;;
  esac
}

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${path}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${path}" | awk '{print $1}'
  else
    die "sha256sum or shasum is required to verify the download"
  fi
}

parse_bootstrap() {
  local json_path="$1"
  local platform="$2"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "${json_path}" "${platform}" <<'PY'
import json, sys
path, platform = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as f:
    data = json.load(f)
entry = (data.get("platforms") or {}).get(platform)
if not entry:
    keys = ", ".join(sorted((data.get("platforms") or {}).keys())) or "(none)"
    raise SystemExit(f"no bootstrap entry for {platform}; available: {keys}")
for key in ("url", "fileName", "sha256", "version", "artifactKind"):
    print(entry.get(key) or "")
PY
    return 0
  fi
  if command -v jq >/dev/null 2>&1; then
    jq -er --arg p "${platform}" '.platforms[$p] // empty | .url' "${json_path}" >/dev/null \
      || die "no bootstrap entry for ${platform}"
    jq -r --arg p "${platform}" '
      .platforms[$p] | [.url, .fileName, (.sha256 // ""), (.version // ""), (.artifactKind // "")] | .[]
    ' "${json_path}"
    return 0
  fi
  die "python3 or jq is required to read installer-bootstrap.json"
}

# Bash 3.2-safe (macOS /bin/bash): read N lines into vars.
read_bootstrap_meta() {
  local json_path="$1"
  local platform="$2"
  local line i=0
  URL="" FILE_NAME="" SHA256="" VERSION="" KIND=""
  while IFS= read -r line; do
    i=$((i + 1))
    case "${i}" in
      1) URL="${line}" ;;
      2) FILE_NAME="${line}" ;;
      3) SHA256="${line}" ;;
      4) VERSION="${line}" ;;
      5) KIND="${line}" ;;
    esac
  done < <(parse_bootstrap "${json_path}" "${platform}")
}

open_installer() {
  local path="$1"
  local base ext
  base="$(basename "${path}")"
  ext="${base##*.}"
  case "$(uname -s)" in
    Darwin)
      log "opening ${base}"
      open "${path}"
      ;;
    Linux)
      case "${ext}" in
        AppImage)
          chmod +x "${path}"
          log "launching ${base}"
          nohup "${path}" >/dev/null 2>&1 &
          ;;
        deb)
          if command -v xdg-open >/dev/null 2>&1; then
            log "opening ${base} (install with your package manager if needed)"
            xdg-open "${path}" >/dev/null 2>&1 || true
          fi
          log "or: sudo dpkg -i \"${path}\""
          ;;
        *)
          if command -v xdg-open >/dev/null 2>&1; then
            xdg-open "${path}" >/dev/null 2>&1 || true
          else
            log "downloaded ${path}"
          fi
          ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      log "starting ${base}"
      cmd.exe /C start "" "$(cygpath -w "${path}" 2>/dev/null || echo "${path}")"
      ;;
    *)
      log "downloaded ${path} — open it to finish installing"
      ;;
  esac
}

PLATFORM="$(bootstrap_platform)"
if [[ -z "${BOOTSTRAP_URL}" ]]; then
  BOOTSTRAP_URL="${BOOTSTRAP_BASE%/}/${CHANNEL}/installer-bootstrap.json"
fi

if [[ -z "${OUT_DIR}" ]]; then
  if [[ -d "${HOME}/Downloads" ]]; then
    OUT_DIR="${HOME}/Downloads"
  else
    OUT_DIR="${TMPDIR:-/tmp}"
  fi
fi
mkdir -p "${OUT_DIR}"

TMP_JSON="$(mktemp "${TMPDIR:-/tmp}/medousa-bootstrap.XXXXXX.json")"
trap 'rm -f "${TMP_JSON}"' EXIT

log "platform ${PLATFORM}"
log "bootstrap ${BOOTSTRAP_URL}"
curl -fsSL "${BOOTSTRAP_URL}" -o "${TMP_JSON}"

read_bootstrap_meta "${TMP_JSON}" "${PLATFORM}"

[[ -n "${URL}" && -n "${FILE_NAME}" ]] || die "bootstrap entry for ${PLATFORM} is incomplete"

DEST="${OUT_DIR%/}/${FILE_NAME}"
log "Medousa ${VERSION:-?} (${KIND:-app}) → ${DEST}"
curl -fL --progress-bar -o "${DEST}" "${URL}"

if [[ -n "${SHA256}" ]]; then
  got="$(sha256_file "${DEST}")"
  [[ "${got}" == "${SHA256}" ]] || die "checksum mismatch for ${FILE_NAME} (got ${got}, want ${SHA256})"
  log "checksum ok"
fi

if [[ "${DOWNLOAD_ONLY}" -eq 1 ]]; then
  log "download-only: ${DEST}"
  exit 0
fi

open_installer "${DEST}"
log "done — finish any installer prompts, then open Medousa"
