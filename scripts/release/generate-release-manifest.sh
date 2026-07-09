#!/usr/bin/env bash
# Generate release-manifest.json from built release artifacts in dist/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

DIST_DIR=""
VERSION=""
CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
BASE_URL_OVERRIDE=""

usage() {
  cat <<'EOF'
Usage: scripts/release/generate-release-manifest.sh [options]

Options:
  --dist <dir>          Directory containing release archives (default: dist/)
  --version <version>   Release version without v prefix (default: Cargo.toml)
  --channel <name>      Release channel (default: stable)
  --base-url <url>      Artifact base URL (or set MEDOUSA_RELEASE_BASE_URL)
  -h, --help            Show this help

Writes dist/release-manifest.json indexing package id → url, sha256, size, depends.
EOF
}

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

sha_for() {
  local file="$1"
  if [[ -f "${DIST_DIR}/SHA256SUMS" ]]; then
    awk -v f="${file}" '$2 == f {print $1; exit}' "${DIST_DIR}/SHA256SUMS"
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

binaries_json_for() {
  local package_id="$1"
  local -a bins
  read -r -a bins <<<"$(medousa_component_binaries "${package_id}" 2>/dev/null || true)"
  if ((${#bins[@]} == 0)); then
    echo "[]"
    return 0
  fi
  local out="["
  local i=0
  for bin in "${bins[@]}"; do
    [[ "${i}" -gt 0 ]] && out+=","
    out+="\"${bin}\""
    i=$((i + 1))
  done
  out+="]"
  echo "${out}"
}

append_package_json() {
  local id="$1"
  local display_name="$2"
  local target="$3"
  local archive="$4"
  local depends_csv="$5"
  local backend="${6:-}"
  local category="${7:-}"
  local url="${BASE_URL}/${archive}"
  local sha256
  sha256="$(sha_for "${archive}")"
  if [[ -z "${sha256}" && -f "${DIST_DIR}/${archive}" ]]; then
    sha256="$(medousa_sha256_file "${DIST_DIR}/${archive}")"
  fi
  local size_bytes
  size_bytes="$(size_for "${DIST_DIR}/${archive}")"
  local depends_json="[]"
  if [[ -n "${depends_csv}" ]]; then
    depends_json="$(printf '%s' "${depends_csv}" | awk -F, '{
      printf "[";
      for (i=1;i<=NF;i++) {
        if (i>1) printf ",";
        printf "\"%s\"", $i
      }
      printf "]";
    }')"
  fi
  local backend_field=""
  if [[ -n "${backend}" ]]; then
    backend_field=$(printf ',\n      "backend": "%s"' "${backend}")
  fi
  local category_field=""
  if [[ -n "${category}" ]]; then
    category_field=$(printf ',\n      "category": "%s"' "${category}")
  fi
  local binaries_field=""
  local binaries_json
  binaries_json="$(binaries_json_for "${id}")"
  if [[ "${binaries_json}" != "[]" ]]; then
    binaries_field=$(printf ',\n      "binaries": %s' "${binaries_json}")
  fi
  cat <<EOF
    "${id}-${target}": {
      "id": "${id}",
      "displayName": "${display_name}",
      "version": "${VERSION}",
      "target": "${target}",
      "url": "${url}",
      "sha256": "${sha256}",
      "sizeBytes": ${size_bytes},
      "depends": ${depends_json}${backend_field}${category_field}${binaries_field}
    }
EOF
}

TARGETS=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  aarch64-apple-darwin
  x86_64-apple-darwin
  x86_64-pc-windows-msvc
)

COMPONENT_DISPLAY_NAMES=(
  "engine:Medousa Engine"
  "cli:Command-line tools"
  "adapter-telegram:Telegram adapter"
  "adapter-discord:Discord adapter"
  "adapter-slack:Slack adapter"
  "adapter-whatsapp:WhatsApp adapter"
  "mcp-gateway:MCP gateway"
)

OUT="${DIST_DIR}/release-manifest.json"
PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

{
  echo "{"
  echo '  "schemaVersion": 2,'
  echo '  "product": "medousa",'
  echo "  \"version\": \"${VERSION}\","
  echo "  \"channel\": \"${CHANNEL}\","
  echo "  \"publishedAt\": \"${PUBLISHED_AT}\","
  echo "  \"baseUrl\": \"${BASE_URL}\","
  echo '  "packages": {'

  first=1
  for target in "${TARGETS[@]}"; do
    for entry in "${COMPONENT_DISPLAY_NAMES[@]}"; do
      package_id="${entry%%:*}"
      display_name="${entry#*:}"
      archive="$(medousa_component_archive_name "${package_id}" "${VERSION}" "${target}")"
      if [[ -f "${DIST_DIR}/${archive}" ]]; then
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        depends="$(medousa_component_depends "${package_id}")"
        category="$(medousa_component_category "${package_id}")"
        append_package_json "${package_id}" "${display_name}" "${target}" "${archive}" "${depends}" "" "${category}"
      fi
    done

    suite_archive="medousa-v${VERSION}-${target}.tar.gz"
    if [[ -f "${DIST_DIR}/${suite_archive}" ]]; then
      [[ "${first}" -eq 1 ]] || echo ","
      first=0
      append_package_json "engine-suite" "Medousa Engine (full suite)" "${target}" "${suite_archive}" "" "" "core"
    fi

    for backend in auto metal cpu cuda; do
      local_archive="medousa_local-${backend}-v${VERSION}-${target}.tar.gz"
      if [[ -f "${DIST_DIR}/${local_archive}" ]]; then
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        append_package_json "local-brain" "Offline brain (${backend})" "${target}" "${local_archive}" "engine" "${backend}" "core"
      fi
    done

    for model in model-gemma-e2b model-gemma-e4b model-gemma-12b; do
      model_archive="${model}-v${VERSION}.manifest.json"
      if [[ -f "${DIST_DIR}/${model_archive}" ]]; then
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        append_package_json "${model}" "${model}" "any" "${model_archive}" "local-brain" "" "model"
      fi
    done
  done

  for name in macos-aarch64 macos-x64 windows-x64 linux-x64; do
    found="$(medousa_desktop_bundle_for_platform "${DIST_DIR}" "${name}" || true)"
    if [[ -n "${found}" ]]; then
      archive="$(basename "${found}")"
      [[ "${first}" -eq 1 ]] || echo ","
      first=0
      append_package_json "desktop" "Medousa Desktop (${name})" "${name}" "${archive}" "" "" "core"
    fi
  done

  for name in macos-aarch64 macos-x64 windows-x64 linux-x64; do
    found="$(medousa_installer_bundle_for_platform "${DIST_DIR}" "${name}" || true)"
    if [[ -n "${found}" ]]; then
      archive="$(basename "${found}")"
      [[ "${first}" -eq 1 ]] || echo ","
      first=0
      append_package_json "installer" "Medousa Installer (${name})" "${name}" "${archive}" "" "" "core"
    fi
  done

  echo ""
  echo "  }"
  echo "}"
} >"${OUT}"

medousa_assert_release_manifest_nonempty "${OUT}"

medousa_log "wrote ${OUT}"
