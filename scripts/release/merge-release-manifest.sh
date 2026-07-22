#!/usr/bin/env bash
# Merge a delta release-manifest.json into an existing channel manifest.
# New package keys win; untouched keys are preserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

BASE_MANIFEST=""
DELTA_MANIFEST=""
OUT=""
CHANNEL_HEAD=""

usage() {
  cat <<'EOF'
Usage: scripts/release/merge-release-manifest.sh [options]

Options:
  --base <file>         Existing channel release-manifest.json (optional)
  --delta <file>        Newly generated manifest covering only shipped packages
  --out <file>          Output path (default: overwrite --delta)
  --channel-head <ver>  Force top-level version (default: max of base + delta)
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base) BASE_MANIFEST="$2"; shift 2 ;;
    --delta) DELTA_MANIFEST="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --channel-head) CHANNEL_HEAD="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

[[ -n "${DELTA_MANIFEST}" && -f "${DELTA_MANIFEST}" ]] || {
  echo "error: --delta must point to an existing manifest" >&2
  exit 1
}
OUT="${OUT:-${DELTA_MANIFEST}}"
medousa_require_cmd jq

if [[ -z "${BASE_MANIFEST}" || ! -f "${BASE_MANIFEST}" ]]; then
  medousa_log "no base manifest — writing delta as-is to ${OUT}"
  if [[ -n "${CHANNEL_HEAD}" ]]; then
    jq --arg v "${CHANNEL_HEAD}" '.version = $v' "${DELTA_MANIFEST}" >"${OUT}.tmp"
    mv "${OUT}.tmp" "${OUT}"
  elif [[ "${OUT}" != "${DELTA_MANIFEST}" ]]; then
    cp -f "${DELTA_MANIFEST}" "${OUT}"
  fi
  exit 0
fi

BASE_VER="$(jq -r '.version // "0.0.0"' "${BASE_MANIFEST}")"
DELTA_VER="$(jq -r '.version // "0.0.0"' "${DELTA_MANIFEST}")"
if [[ -n "${CHANNEL_HEAD}" ]]; then
  HEAD="${CHANNEL_HEAD}"
else
  HEAD="$(medousa_semver_max "${BASE_VER}" "${DELTA_VER}")"
  # Also consider per-package versions in both manifests.
  while IFS= read -r pv; do
    [[ -n "${pv}" && "${pv}" != "null" ]] || continue
    HEAD="$(medousa_semver_max "${HEAD}" "${pv}")"
  done < <(jq -r '.packages // {} | .[].version // empty' "${BASE_MANIFEST}" "${DELTA_MANIFEST}")
fi

PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

jq -n \
  --slurpfile base "${BASE_MANIFEST}" \
  --slurpfile delta "${DELTA_MANIFEST}" \
  --arg head "${HEAD}" \
  --arg published "${PUBLISHED_AT}" \
  '
  ($base[0] // {}) as $b
  | ($delta[0] // {}) as $d
  | {
      schemaVersion: ($d.schemaVersion // $b.schemaVersion // 2),
      product: ($d.product // $b.product // "medousa"),
      version: $head,
      channel: ($d.channel // $b.channel // "stable"),
      publishedAt: $published,
      baseUrl: ($d.baseUrl // $b.baseUrl // ""),
      packages: (($b.packages // {}) + ($d.packages // {}))
    }
  ' >"${OUT}.tmp"
mv "${OUT}.tmp" "${OUT}"

medousa_assert_release_manifest_nonempty "${OUT}"
medousa_log "merged manifest → ${OUT} (channel head ${HEAD}, packages=$(jq '.packages | length' "${OUT}"))"
