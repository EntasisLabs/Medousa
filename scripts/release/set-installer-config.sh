#!/usr/bin/env bash
# Write apps/medousa-installer/public/installer-config.json from env (baked into the installer at build).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

BASE_URL="${MEDOUSA_RELEASE_BASE_URL:-}"
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"

usage() {
  cat <<'EOF'
Usage: scripts/release/set-installer-config.sh

Writes installer-config.json from:
  MEDOUSA_RELEASE_BASE_URL   (required for production builds)
  MEDOUSA_RELEASE_CHANNEL    (default: stable)

Run before `npm run tauri build` in apps/medousa-installer.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

ROOT="$(medousa_repo_root)"
OUT="${ROOT}/apps/medousa-installer/public/installer-config.json"

if [[ -z "${BASE_URL}" ]]; then
  echo "warning: MEDOUSA_RELEASE_BASE_URL is empty — installer will fall back to GitHub Releases" >&2
fi

mkdir -p "$(dirname "${OUT}")"
cat >"${OUT}" <<EOF
{
  "releaseBaseUrl": "${BASE_URL%/}",
  "releaseChannel": "${CHANNEL}",
  "bootstrapPath": "installer-bootstrap.json",
  "manifestPath": "release-manifest.json"
}
EOF

medousa_log "wrote ${OUT} (base=${BASE_URL:-<empty>} channel=${CHANNEL})"
