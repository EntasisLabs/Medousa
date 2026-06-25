#!/usr/bin/env bash
# Medousa release scripts — shared constants and helpers.
# Source from other scripts: source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

set -euo pipefail

MEDOUSA_GITHUB_REPO="${MEDOUSA_GITHUB_REPO:-EntasisLabs/Medousa}"
MEDOUSA_RELEASE_BASE_URL="${MEDOUSA_RELEASE_BASE_URL:-}"
MEDOUSA_RELEASE_CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"

# All binaries shipped in every platform tarball (same directory for sibling resolution).
MEDOUSA_BINARIES=(
  medousa
  medousa_cli
  medousa_daemon
  medousa_local
  medousa_tui
  medousa_telegram
  medousa_discord
  medousa_slack
  medousa_mcp_gateway
  medousa_whatsapp
)

MEDOUSA_MAIN_CARGO_TOML="${MEDOUSA_MAIN_CARGO_TOML:-Cargo.toml}"
MEDOUSA_WHATSAPP_CARGO_TOML="${MEDOUSA_WHATSAPP_CARGO_TOML:-adapters/medousa-whatsapp/Cargo.toml}"
MEDOUSA_WHATSAPP_MANIFEST="${MEDOUSA_WHATSAPP_MANIFEST:-adapters/medousa-whatsapp/Cargo.toml}"

medousa_repo_root() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  echo "${script_dir}"
}

medousa_parse_cargo_version() {
  local toml_path="$1"
  sed -n 's/^version = "\(.*\)"/\1/p' "${toml_path}" | head -1
}

medousa_version() {
  medousa_parse_cargo_version "$(medousa_repo_root)/${MEDOUSA_MAIN_CARGO_TOML}"
}

medousa_whatsapp_version() {
  medousa_parse_cargo_version "$(medousa_repo_root)/${MEDOUSA_WHATSAPP_CARGO_TOML}"
}

medousa_assert_versions_match() {
  local root_v wa_v
  root_v="$(medousa_version)"
  wa_v="$(medousa_whatsapp_version)"
  if [[ "${root_v}" != "${wa_v}" ]]; then
    echo "error: version mismatch — root Cargo.toml (${root_v}) != whatsapp (${wa_v})" >&2
    exit 1
  fi
}

medousa_tag_for_version() {
  local version="${1:-$(medousa_version)}"
  echo "v${version}"
}

medousa_version_from_tag() {
  local tag="$1"
  tag="${tag#v}"
  echo "${tag}"
}

medousa_host_target() {
  rustc -vV | sed -n 's/^host: //p'
}

medousa_is_windows_msvc_target() {
  [[ "$1" == *"-pc-windows-msvc" ]]
}

medousa_binary_filename() {
  local name="$1"
  local target="$2"
  if medousa_is_windows_msvc_target "${target}"; then
    echo "${name}.exe"
  else
    echo "${name}"
  fi
}

medousa_asset_basename() {
  local version="${1:-$(medousa_version)}"
  local target="$2"
  echo "medousa-v${version}-${target}"
}

medousa_asset_archive_name() {
  echo "$(medousa_asset_basename "$1" "$2").tar.gz"
}

# Component package IDs (separate release artifacts).
MEDOUSA_COMPONENT_IDS=(
  engine
  cli
  adapter-telegram
  adapter-discord
  adapter-slack
  adapter-whatsapp
  mcp-gateway
)

medousa_component_binaries() {
  case "$1" in
    engine) echo "medousa medousa_daemon" ;;
    cli) echo "medousa_cli medousa_tui" ;;
    adapter-telegram) echo "medousa_telegram" ;;
    adapter-discord) echo "medousa_discord" ;;
    adapter-slack) echo "medousa_slack" ;;
    adapter-whatsapp) echo "medousa_whatsapp" ;;
    mcp-gateway) echo "medousa_mcp_gateway" ;;
    *)
      echo "error: unknown component package: $1" >&2
      return 1
      ;;
  esac
}

medousa_component_category() {
  case "$1" in
    engine | cli) echo "core" ;;
    adapter-*) echo "adapter" ;;
    mcp-gateway) echo "core" ;;
    local-brain) echo "core" ;;
    desktop | installer) echo "core" ;;
    model-*) echo "model" ;;
    grapheme-* | skill-hub) echo "expansion" ;;
    *) echo "core" ;;
  esac
}

medousa_component_depends() {
  case "$1" in
    engine | desktop | installer) echo "" ;;
    cli | adapter-* | mcp-gateway | local-brain) echo "engine" ;;
    model-*) echo "local-brain" ;;
    *) echo "" ;;
  esac
}

medousa_component_basename() {
  local package_id="$1"
  local version="$2"
  local target="$3"
  echo "${package_id}-v${version}-${target}"
}

medousa_component_archive_name() {
  echo "$(medousa_component_basename "$1" "$2" "$3").tar.gz"
}

medousa_release_base_url() {
  local channel="${MEDOUSA_RELEASE_CHANNEL:-stable}"
  local base="${MEDOUSA_RELEASE_BASE_URL:-}"
  if [[ -n "${base}" ]]; then
    echo "${base%/}/${channel}"
    return 0
  fi
  local version="${1:-$(medousa_version)}"
  echo "https://github.com/${MEDOUSA_GITHUB_REPO}/releases/download/v${version}"
}

medousa_release_manifest_url() {
  if [[ -n "${MEDOUSA_RELEASE_MANIFEST_URL:-}" ]]; then
    echo "${MEDOUSA_RELEASE_MANIFEST_URL}"
    return 0
  fi
  echo "$(medousa_release_base_url)/release-manifest.json"
}

medousa_release_bootstrap_url() {
  echo "$(medousa_release_base_url)/installer-bootstrap.json"
}

medousa_component_set_id_for_binaries() {
  local bin_dir="$1"
  local target="$2"
  shift 2
  local bins=("$@")
  local bin file path tmp result
  tmp="$(mktemp)"
  for bin in "${bins[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    path="${bin_dir}/${file}"
    if [[ ! -f "${path}" ]]; then
      rm -f "${tmp}"
      echo "error: missing binary for component set: ${path}" >&2
      return 1
    fi
    medousa_sha256_file "${path}" >>"${tmp}"
  done
  result="$(medousa_sha256_file "${tmp}")"
  rm -f "${tmp}"
  echo "${result}"
}

medousa_write_component_install_manifest() {
  local bin_dir="$1"
  local package_id="$2"
  local version="$3"
  local target="$4"
  local out_path="$5"
  local built_at="${6:-$(date -u +"%Y-%m-%dT%H:%M:%SZ")}"
  local -a bins
  read -r -a bins <<<"$(medousa_component_binaries "${package_id}")"
  local component_set_id
  component_set_id="$(medousa_component_set_id_for_binaries "${bin_dir}" "${target}" "${bins[@]}")"

  local bin_list=""
  local bin
  for bin in "${bins[@]}"; do
    if [[ -n "${bin_list}" ]]; then
      bin_list+=", "
    fi
    bin_list+="\"${bin}\""
  done

  cat >"${out_path}" <<EOF
{
  "schema_version": 2,
  "product": "medousa",
  "package_id": "${package_id}",
  "version": "${version}",
  "target": "${target}",
  "built_at": "${built_at}",
  "binaries": [${bin_list}],
  "component_set_id": "${component_set_id}"
}
EOF
}

medousa_cargo_target_root() {
  local root
  root="$(medousa_repo_root)"
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "${CARGO_TARGET_DIR}"
  else
    echo "${root}/target"
  fi
}

medousa_whatsapp_cargo_target_root() {
  local root
  root="$(medousa_repo_root)"
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "${CARGO_TARGET_DIR}"
  else
    echo "${root}/adapters/medousa-whatsapp/target"
  fi
}

medousa_cargo_release_dir() {
  local target="${1:-}"
  local base
  base="$(medousa_cargo_target_root)"
  if [[ -n "${target}" ]]; then
    echo "${base}/${target}/release"
  else
    echo "${base}/release"
  fi
}

medousa_whatsapp_cargo_release_dir() {
  local target="${1:-}"
  local base
  base="$(medousa_whatsapp_cargo_target_root)"
  if [[ -n "${target}" ]]; then
    echo "${base}/${target}/release"
  else
    echo "${base}/release"
  fi
}

# Resolve a built binary across root vs whatsapp target dirs (CARGO_TARGET_DIR, cross-target).
medousa_find_release_binary() {
  local bin="$1"
  local target="$2"
  local file
  file="$(medousa_binary_filename "${bin}" "${target}")"
  local candidate
  for candidate in \
    "$(medousa_cargo_release_dir "${target}")/${file}" \
    "$(medousa_whatsapp_cargo_release_dir "${target}")/${file}" \
    "$(medousa_repo_root)/target/release/${file}" \
    "$(medousa_repo_root)/target/${target}/release/${file}"; do
    if [[ -f "${candidate}" ]]; then
      echo "${candidate}"
      return 0
    fi
  done
  return 1
}

# Map uname-style OS/arch to Rust triple for install.sh (must stay in sync).
medousa_install_target_from_uname() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Linux:x86_64)  echo "x86_64-unknown-linux-gnu" ;;
    Linux:aarch64|Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
    Darwin:arm64|Darwin:aarch64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*:x86_64) echo "x86_64-pc-windows-msvc" ;;
    *)
      echo "error: unsupported platform ${os}/${arch}" >&2
      return 1
      ;;
  esac
}

medousa_log() {
  echo "[medousa-release] $*"
}

medousa_require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "error: required command not found: ${cmd}" >&2
    exit 1
  fi
}

medousa_sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${path}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${path}" | awk '{print $1}'
  else
    echo "error: sha256sum or shasum required" >&2
    return 1
  fi
}

# Fingerprint of the full binary set — detects partial or mismatched installs.
medousa_component_set_id() {
  local bin_dir="$1"
  local target="$2"
  local bin file path tmp result
  tmp="$(mktemp)"
  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    path="${bin_dir}/${file}"
    if [[ ! -f "${path}" ]]; then
      rm -f "${tmp}"
      echo "error: missing binary for component set: ${path}" >&2
      return 1
    fi
    medousa_sha256_file "${path}" >>"${tmp}"
  done
  result="$(medousa_sha256_file "${tmp}")"
  rm -f "${tmp}"
  echo "${result}"
}

medousa_write_install_manifest() {
  local bin_dir="$1"
  local version="$2"
  local target="$3"
  local out_path="$4"
  local built_at="${5:-$(date -u +"%Y-%m-%dT%H:%M:%SZ")}"
  local component_set_id
  component_set_id="$(medousa_component_set_id "${bin_dir}" "${target}")"

  local bin_list=""
  local bin
  for bin in "${MEDOUSA_BINARIES[@]}"; do
    if [[ -n "${bin_list}" ]]; then
      bin_list+=", "
    fi
    bin_list+="\"${bin}\""
  done

  cat >"${out_path}" <<EOF
{
  "schema_version": 1,
  "product": "medousa",
  "version": "${version}",
  "target": "${target}",
  "built_at": "${built_at}",
  "binaries": [${bin_list}],
  "component_set_id": "${component_set_id}"
}
EOF
}

medousa_read_manifest_field() {
  local manifest_path="$1"
  local field="$2"
  sed -n "s/.*\"${field}\": \"\\([^\"]*\\)\".*/\\1/p" "${manifest_path}" | head -1
}
