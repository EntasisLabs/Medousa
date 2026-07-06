#!/usr/bin/env bash
# Install APNs credentials into the Medousa data directory for official builds.
# End users never run this — release engineering runs it once per Mac host (or in CI).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXAMPLE_CONFIG="${REPO_ROOT}/config/apns/config.example.json"

TEAM_ID=""
KEY_ID=""
KEY_FILE=""
BUNDLE_ID="com.entasislabs.medousa-home"
SANDBOX="true"
DATA_DIR="${MEDOUSA_DATA_DIR:-}"
KEY_STORAGE="keychain"

usage() {
  cat <<EOF
Usage: install-apns-push.sh [options]

Install APNs credentials for remote push (Medousa Home iOS).

On macOS the Auth Key (.p8) is stored in Keychain (service: medousa.apns).
Metadata lives in {medousa_data_dir}/apns/config.json — no .p8 on disk.

Options:
  --team-id <id>        Apple Developer Team ID (required)
  --key-id <id>         APNs Auth Key ID (required)
  --key-file <path>     Path to AuthKey_*.p8 (required)
  --bundle-id <id>      iOS bundle ID (default: com.entasislabs.medousa-home)
  --production          Use production APNs endpoint (default: sandbox)
  --data-dir <path>     Override MEDOUSA_DATA_DIR
  --file-storage        Store .p8 on disk instead of Keychain (Linux / fallback)
  -h, --help            Show this help

Example:
  ./scripts/install-apns-push.sh \\
    --team-id XXXXXXXXXX \\
    --key-id YYYYYYYYYY \\
    --key-file ~/Downloads/AuthKey_YYYYYYYYYY.p8

Restart medousa_daemon after install.
See docs/runbooks/mobile-push-deployment.md for the full release checklist.
EOF
}

log() {
  echo "[install-apns-push] $*"
}

store_apns_key_in_keychain() {
  local pem="$1"
  security delete-generic-password -s "medousa.apns" -a "auth_key" >/dev/null 2>&1 || true
  security add-generic-password \
    -s "medousa.apns" \
    -a "auth_key" \
    -w "$pem" \
    -U \
    >/dev/null
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --team-id)
      TEAM_ID="$2"
      shift 2
      ;;
    --key-id)
      KEY_ID="$2"
      shift 2
      ;;
    --key-file)
      KEY_FILE="$2"
      shift 2
      ;;
    --bundle-id)
      BUNDLE_ID="$2"
      shift 2
      ;;
    --production)
      SANDBOX="false"
      shift
      ;;
    --data-dir)
      DATA_DIR="$2"
      shift 2
      ;;
    --file-storage)
      KEY_STORAGE="file"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "${TEAM_ID}" || -z "${KEY_ID}" || -z "${KEY_FILE}" ]]; then
  echo "error: --team-id, --key-id, and --key-file are required" >&2
  usage >&2
  exit 1
fi

if [[ ! -f "${KEY_FILE}" ]]; then
  echo "error: key file not found: ${KEY_FILE}" >&2
  exit 1
fi

if [[ -z "${DATA_DIR}" ]]; then
  if [[ "$(uname -s)" == "Darwin" ]]; then
    DATA_DIR="${HOME}/Library/Application Support/medousa"
  else
    DATA_DIR="${HOME}/.local/share/medousa"
  fi
fi

if [[ "${KEY_STORAGE}" == "keychain" && "$(uname -s)" != "Darwin" ]]; then
  log "Keychain storage is macOS-only; using --file-storage"
  KEY_STORAGE="file"
fi

APNS_DIR="${DATA_DIR}/apns"
mkdir -p "${APNS_DIR}"
chmod 700 "${APNS_DIR}"

KEY_BASENAME="$(basename "${KEY_FILE}")"
KEY_PEM="$(cat "${KEY_FILE}")"

KEY_FILE_JSON=""
if [[ "${KEY_STORAGE}" == "keychain" ]]; then
  store_apns_key_in_keychain "${KEY_PEM}"
  log "Stored APNs key in Keychain (service medousa.apns, account auth_key)"
else
  DEST_KEY="${APNS_DIR}/${KEY_BASENAME}"
  cp "${KEY_FILE}" "${DEST_KEY}"
  chmod 600 "${DEST_KEY}"
  KEY_FILE_JSON="\"keyFile\": \"${KEY_BASENAME}\","
  log "Stored APNs key at ${DEST_KEY}"
fi

CONFIG_PATH="${APNS_DIR}/config.json"
cat >"${CONFIG_PATH}" <<EOF
{
  "teamId": "${TEAM_ID}",
  "keyId": "${KEY_ID}",
  ${KEY_FILE_JSON}
  "keyStorage": "${KEY_STORAGE}",
  "bundleId": "${BUNDLE_ID}",
  "sandbox": ${SANDBOX}
}
EOF
chmod 600 "${CONFIG_PATH}"

log "Installed APNs config to ${APNS_DIR}"
log "  config: ${CONFIG_PATH}"
log "Restart medousa_daemon to pick up credentials."
log "Example template: ${EXAMPLE_CONFIG}"
