#!/usr/bin/env bash
# Merge a delta installer-bootstrap.json into an existing channel bootstrap.
# Platforms present in the delta replace the base; others are kept.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

BASE_BOOTSTRAP=""
DELTA_BOOTSTRAP=""
OUT=""
CHANNEL_HEAD=""

usage() {
  cat <<'EOF'
Usage: scripts/release/merge-installer-bootstrap.sh [options]

Options:
  --base <file>         Existing channel installer-bootstrap.json (optional)
  --delta <file>        Newly generated bootstrap covering shipped platforms
  --out <file>          Output path (default: overwrite --delta)
  --channel-head <ver>  Force top-level version
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base) BASE_BOOTSTRAP="$2"; shift 2 ;;
    --delta) DELTA_BOOTSTRAP="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --channel-head) CHANNEL_HEAD="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

[[ -n "${DELTA_BOOTSTRAP}" && -f "${DELTA_BOOTSTRAP}" ]] || {
  echo "error: --delta must point to an existing bootstrap" >&2
  exit 1
}
OUT="${OUT:-${DELTA_BOOTSTRAP}}"
medousa_require_cmd jq

if [[ -z "${BASE_BOOTSTRAP}" || ! -f "${BASE_BOOTSTRAP}" ]]; then
  medousa_log "no base bootstrap — writing delta as-is to ${OUT}"
  if [[ -n "${CHANNEL_HEAD}" ]]; then
    jq --arg v "${CHANNEL_HEAD}" '.version = $v' "${DELTA_BOOTSTRAP}" >"${OUT}.tmp"
    mv "${OUT}.tmp" "${OUT}"
  elif [[ "${OUT}" != "${DELTA_BOOTSTRAP}" ]]; then
    cp -f "${DELTA_BOOTSTRAP}" "${OUT}"
  fi
  exit 0
fi

BASE_VER="$(jq -r '.version // "0.0.0"' "${BASE_BOOTSTRAP}")"
DELTA_VER="$(jq -r '.version // "0.0.0"' "${DELTA_BOOTSTRAP}")"
HEAD="${CHANNEL_HEAD:-$(medousa_semver_max "${BASE_VER}" "${DELTA_VER}")}"
PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

jq -n \
  --slurpfile base "${BASE_BOOTSTRAP}" \
  --slurpfile delta "${DELTA_BOOTSTRAP}" \
  --arg head "${HEAD}" \
  --arg published "${PUBLISHED_AT}" \
  '
  ($base[0] // {}) as $b
  | ($delta[0] // {}) as $d
  | {
      schemaVersion: ($d.schemaVersion // $b.schemaVersion // 1),
      product: ($d.product // $b.product // "medousa"),
      version: $head,
      channel: ($d.channel // $b.channel // "stable"),
      publishedAt: $published,
      manifestUrl: ($d.manifestUrl // $b.manifestUrl // ""),
      platforms: (($b.platforms // {}) + ($d.platforms // {}))
    }
  ' >"${OUT}.tmp"
mv "${OUT}.tmp" "${OUT}"

# Allow empty platforms only if base was also empty (first publish edge case).
if [[ "$(jq '.platforms | length' "${OUT}")" -lt 1 ]]; then
  echo "error: merged bootstrap has no platforms" >&2
  exit 1
fi

medousa_log "merged bootstrap → ${OUT} (channel head ${HEAD}, platforms=$(jq '.platforms | length' "${OUT}"))"
