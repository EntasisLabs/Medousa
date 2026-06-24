#!/usr/bin/env bash
# Generate release-manifest.json from built release artifacts in dist/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

DIST_DIR=""
VERSION=""
GITHUB_REPO="${MEDOUSA_GITHUB_REPO:-EntasisLabs/Medousa}"

usage() {
  cat <<'EOF'
Usage: scripts/release/generate-release-manifest.sh [options]

Options:
  --dist <dir>          Directory containing release archives (default: dist/)
  --version <version>   Release version without v prefix (default: Cargo.toml)
  -h, --help            Show this help

Writes dist/release-manifest.json indexing package id → url, sha256, size, depends.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dist) DIST_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

ROOT="$(medousa_repo_root)"
cd "${ROOT}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"
VERSION="${VERSION:-$(medousa_version)}"
TAG="v${VERSION}"
BASE_URL="https://github.com/${GITHUB_REPO}/releases/download/${TAG}"

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

append_package_json() {
  local id="$1"
  local display_name="$2"
  local target="$3"
  local archive="$4"
  local depends_csv="$5"
  local backend="${6:-}"
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
  cat <<EOF
    "${id}-${target}": {
      "id": "${id}",
      "displayName": "${display_name}",
      "version": "${VERSION}",
      "target": "${target}",
      "url": "${url}",
      "sha256": "${sha256}",
      "sizeBytes": ${size_bytes},
      "depends": ${depends_json}${backend_field}
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

OUT="${DIST_DIR}/release-manifest.json"
PUBLISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

{
  echo "{"
  echo '  "schemaVersion": 1,'
  echo '  "product": "medousa",'
  echo "  \"version\": \"${VERSION}\","
  echo "  \"publishedAt\": \"${PUBLISHED_AT}\","
  echo '  "packages": {'

  first=1
  for target in "${TARGETS[@]}"; do
    cli_archive="medousa-v${VERSION}-${target}.tar.gz"
    if [[ -f "${DIST_DIR}/${cli_archive}" ]]; then
      [[ "${first}" -eq 1 ]] || echo ","
      first=0
      append_package_json "engine" "Medousa Engine" "${target}" "${cli_archive}" ""
    fi

    for backend in metal cpu cuda; do
      local_archive="medousa_local-${backend}-v${VERSION}-${target}.tar.gz"
      if [[ -f "${DIST_DIR}/${local_archive}" ]]; then
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        append_package_json "local-brain" "Offline brain (${backend})" "${target}" "${local_archive}" "engine" "${backend}"
      fi
    done

    for model in model-gemma-e2b model-gemma-e4b model-gemma-12b; do
      model_archive="${model}-v${VERSION}.manifest.json"
      if [[ -f "${DIST_DIR}/${model_archive}" ]]; then
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        append_package_json "${model}" "${model}" "any" "${model_archive}" "local-brain"
      fi
    done
  done

  for name in macos-aarch64 windows-x64 linux-x64; do
    for ext in dmg msi exe AppImage deb; do
      found="$(find "${DIST_DIR}" -maxdepth 3 -type f -name "*.${ext}" 2>/dev/null | head -1 || true)"
      if [[ -n "${found}" ]]; then
        archive="$(basename "${found}")"
        [[ "${first}" -eq 1 ]] || echo ","
        first=0
        append_package_json "desktop" "Medousa Desktop (${name})" "${name}" "${archive}" ""
        break
      fi
    done
  done

  echo ""
  echo "  }"
  echo "}"
} >"${OUT}"

medousa_log "wrote ${OUT}"
