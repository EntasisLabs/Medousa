#!/usr/bin/env bash
# Fetch medousa_daemon (+ launcher) from a published engine package on R2/CDN.
# Used so desktop-only releases can skip the expensive daemon rebuild matrix.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
OUT_DIR=""
ENGINE_VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
BASE_URL_OVERRIDE=""
MANIFEST_URL=""

usage() {
  cat <<'EOF'
Usage: scripts/release/fetch-daemon-from-r2.sh --target <triple> --out-dir <dir> [options]

Downloads the published engine archive for a Rust target and stages:
  medousa_daemon-<target>[.exe]
  medousa-<target>[.exe]
into --out-dir (same layout as release.yml build-daemon artifacts).

Options:
  --target <triple>           Rust target (required)
  --out-dir <dir>             Output directory (required)
  --engine-version <version>  Prefer this engine stamp (default: package-versions.toml
                              engine, else whatever the channel manifest lists)
  --channel <name>            Release channel (default: stable)
  --base-url <url>            Channel base override (…/medousa or …/medousa/stable)
  --manifest-url <url>        Explicit release-manifest.json URL
  -h, --help                  Show this help

Resolution order for the archive URL:
  1. Channel release-manifest.json engine entry for --target (optionally filtered
     by --engine-version)
  2. Direct URL {base}/engine-v{version}-{target}.tar.gz using --engine-version
     or package-versions.toml engine stamp

Public CDN works without R2 credentials. Set AWS_* + MEDOUSA_R2_* only if the
public URL fails and you need an authenticated s3 cp fallback.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --engine-version) ENGINE_VERSION="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --base-url) BASE_URL_OVERRIDE="$2"; shift 2 ;;
    --manifest-url) MANIFEST_URL="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "${TARGET}" || -z "${OUT_DIR}" ]]; then
  echo "error: --target and --out-dir are required" >&2
  exit 1
fi

medousa_require_cmd curl
medousa_require_cmd tar
medousa_require_cmd jq

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

if [[ -z "${ENGINE_VERSION}" ]]; then
  ENGINE_VERSION="$(medousa_package_version engine 2>/dev/null || true)"
fi

if [[ -n "${BASE_URL_OVERRIDE}" ]]; then
  # Accept either …/medousa or …/medousa/stable
  BASE_URL="${BASE_URL_OVERRIDE%/}"
  if [[ "${BASE_URL}" != */"${CHANNEL}" ]]; then
    BASE_URL="${BASE_URL}/${CHANNEL}"
  fi
elif [[ -n "${MEDOUSA_RELEASE_BASE_URL:-}" ]]; then
  MEDOUSA_RELEASE_CHANNEL="${CHANNEL}"
  BASE_URL="$(medousa_release_base_url "${ENGINE_VERSION:-0.0.0}")"
else
  # Prefer the public CDN — GitHub Release assets are not the channel source of truth.
  BASE_URL="https://releases.entasislabs.com/medousa/${CHANNEL}"
fi
BASE_URL="${BASE_URL%/}"
MANIFEST_URL="${MANIFEST_URL:-${BASE_URL}/release-manifest.json}"

EXT=""
[[ "${TARGET}" == *-pc-windows-msvc ]] && EXT=".exe"

mkdir -p "${OUT_DIR}"
OUT_DIR="$(cd "${OUT_DIR}" && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/medousa-fetch-daemon.XXXXXX")"
cleanup() { rm -rf "${WORK}"; }
trap cleanup EXIT

ARCHIVE_URL=""
RESOLVED_VERSION="${ENGINE_VERSION}"

medousa_log "resolving engine daemon for target=${TARGET} channel=${CHANNEL}"
medousa_log "manifest: ${MANIFEST_URL}"

if curl -fsSL "${MANIFEST_URL}" -o "${WORK}/release-manifest.json"; then
  # Prefer exact version pin when provided; otherwise take the channel entry.
  if [[ -n "${ENGINE_VERSION}" ]]; then
    ARCHIVE_URL="$(jq -r --arg t "${TARGET}" --arg v "${ENGINE_VERSION}" '
      .packages // {}
      | to_entries[]
      | select(.value.id == "engine" and .value.target == $t and .value.version == $v)
      | .value.url
    ' "${WORK}/release-manifest.json" | head -1)"
  fi
  if [[ -z "${ARCHIVE_URL}" || "${ARCHIVE_URL}" == "null" ]]; then
    ARCHIVE_URL="$(jq -r --arg t "${TARGET}" '
      .packages // {}
      | to_entries[]
      | select(.value.id == "engine" and .value.target == $t)
      | .value.url
    ' "${WORK}/release-manifest.json" | head -1)"
    RESOLVED_VERSION="$(jq -r --arg t "${TARGET}" '
      .packages // {}
      | to_entries[]
      | select(.value.id == "engine" and .value.target == $t)
      | .value.version
    ' "${WORK}/release-manifest.json" | head -1)"
  fi
  if [[ "${ARCHIVE_URL}" == "null" ]]; then
    ARCHIVE_URL=""
  fi
else
  medousa_log "warning: could not fetch ${MANIFEST_URL}; falling back to direct engine URL"
fi

if [[ -z "${ARCHIVE_URL}" ]]; then
  if [[ -z "${RESOLVED_VERSION}" ]]; then
    echo "error: no engine version available (set --engine-version or package-versions.toml engine)" >&2
    exit 1
  fi
  ARCHIVE_URL="${BASE_URL}/engine-v${RESOLVED_VERSION}-${TARGET}.tar.gz"
fi

ARCHIVE_PATH="${WORK}/engine.tar.gz"
medousa_log "downloading ${ARCHIVE_URL}"
if ! curl -fL --retry 3 --retry-delay 2 -o "${ARCHIVE_PATH}" "${ARCHIVE_URL}"; then
  # Authenticated R2 fallback when the public CDN blocks or the object is private.
  if [[ -n "${AWS_ACCESS_KEY_ID:-}" && -n "${AWS_SECRET_ACCESS_KEY:-}" && -n "${MEDOUSA_R2_BUCKET:-}" ]]; then
    medousa_require_cmd aws
    PREFIX="${MEDOUSA_R2_PREFIX:-medousa/${CHANNEL}}"
    KEY="${PREFIX%/}/$(basename "${ARCHIVE_URL}")"
    ENDPOINT="${MEDOUSA_R2_ENDPOINT:-${AWS_ENDPOINT_URL:-}}"
    AWS_ARGS=(s3 cp "s3://${MEDOUSA_R2_BUCKET}/${KEY}" "${ARCHIVE_PATH}")
    [[ -n "${ENDPOINT}" ]] && AWS_ARGS+=(--endpoint-url "${ENDPOINT}")
    medousa_log "public download failed; trying R2 s3://${MEDOUSA_R2_BUCKET}/${KEY}"
    aws "${AWS_ARGS[@]}"
  else
    echo "error: failed to download ${ARCHIVE_URL}" >&2
    exit 1
  fi
fi

tar -xzf "${ARCHIVE_PATH}" -C "${WORK}"
DAEMON_SRC="$(find "${WORK}" -type f \( -name "medousa_daemon${EXT}" -o -name 'medousa_daemon' \) ! -path '*/.*' | head -1 || true)"
LAUNCHER_SRC="$(find "${WORK}" -type f \( -name "medousa${EXT}" -o -name 'medousa' \) ! -path '*/.*' ! -name 'medousa_daemon*' ! -name 'medousa_cli*' ! -name 'medousa_tui*' | head -1 || true)"

if [[ -z "${DAEMON_SRC}" || ! -f "${DAEMON_SRC}" ]]; then
  echo "error: medousa_daemon${EXT} not found inside $(basename "${ARCHIVE_URL}")" >&2
  find "${WORK}" -type f | sed 's/^/  /' >&2 || true
  exit 1
fi

cp -f "${DAEMON_SRC}" "${OUT_DIR}/medousa_daemon-${TARGET}${EXT}"
chmod +x "${OUT_DIR}/medousa_daemon-${TARGET}${EXT}" || true
if [[ -n "${LAUNCHER_SRC}" && -f "${LAUNCHER_SRC}" ]]; then
  cp -f "${LAUNCHER_SRC}" "${OUT_DIR}/medousa-${TARGET}${EXT}"
  chmod +x "${OUT_DIR}/medousa-${TARGET}${EXT}" || true
fi

medousa_log "staged daemon → ${OUT_DIR}/medousa_daemon-${TARGET}${EXT} (engine v${RESOLVED_VERSION:-unknown})"
ls -la "${OUT_DIR}/medousa_daemon-${TARGET}${EXT}" "${OUT_DIR}/medousa-${TARGET}${EXT}" 2>/dev/null || true
