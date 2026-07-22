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

# Optional binaries: shipped as their own standalone component (e.g. the heavy
# mistralrs offline brain) rather than inside the full-suite tarball. When absent
# from a bin dir they are skipped instead of treated as an error, so the core
# bundle can be built, packaged, and installed without them. When present, they
# are still included and fingerprinted.
MEDOUSA_OPTIONAL_BINARIES=(
  medousa_local
)

medousa_is_optional_binary() {
  local candidate="$1" b
  for b in "${MEDOUSA_OPTIONAL_BINARIES[@]}"; do
    [[ "${b}" == "${candidate}" ]] && return 0
  done
  return 1
}

MEDOUSA_MAIN_CARGO_TOML="${MEDOUSA_MAIN_CARGO_TOML:-Cargo.toml}"
MEDOUSA_WHATSAPP_CARGO_TOML="${MEDOUSA_WHATSAPP_CARGO_TOML:-adapters/medousa-whatsapp/Cargo.toml}"
MEDOUSA_WHATSAPP_MANIFEST="${MEDOUSA_WHATSAPP_MANIFEST:-adapters/medousa-whatsapp/Cargo.toml}"
MEDOUSA_TELEGRAM_MANIFEST="${MEDOUSA_TELEGRAM_MANIFEST:-adapters/medousa-telegram/Cargo.toml}"
MEDOUSA_DISCORD_MANIFEST="${MEDOUSA_DISCORD_MANIFEST:-adapters/medousa-discord/Cargo.toml}"
MEDOUSA_SLACK_MANIFEST="${MEDOUSA_SLACK_MANIFEST:-adapters/medousa-slack/Cargo.toml}"
MEDOUSA_MCP_GATEWAY_MANIFEST="${MEDOUSA_MCP_GATEWAY_MANIFEST:-adapters/medousa-mcp-gateway/Cargo.toml}"

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

medousa_adapter_manifest() {
  case "$1" in
    medousa_telegram) echo "${MEDOUSA_TELEGRAM_MANIFEST}" ;;
    medousa_discord) echo "${MEDOUSA_DISCORD_MANIFEST}" ;;
    medousa_slack) echo "${MEDOUSA_SLACK_MANIFEST}" ;;
    medousa_whatsapp) echo "${MEDOUSA_WHATSAPP_MANIFEST}" ;;
    medousa_mcp_gateway) echo "${MEDOUSA_MCP_GATEWAY_MANIFEST}" ;;
    *)
      echo "error: no adapter manifest for binary: $1" >&2
      return 1
      ;;
  esac
}

medousa_is_adapter_binary() {
  case "$1" in
    medousa_telegram | medousa_discord | medousa_slack | medousa_whatsapp | medousa_mcp_gateway)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

MEDOUSA_PACKAGE_VERSIONS_TOML="${MEDOUSA_PACKAGE_VERSIONS_TOML:-scripts/release/package-versions.toml}"

# All package ids that carry an independent release stamp.
MEDOUSA_PACKAGE_VERSION_IDS=(
  engine
  adapter-telegram
  adapter-discord
  adapter-slack
  adapter-whatsapp
  mcp-gateway
  local-brain
  desktop
  installer
)

medousa_package_versions_path() {
  echo "$(medousa_repo_root)/${MEDOUSA_PACKAGE_VERSIONS_TOML}"
}

# Look up a package's release version from package-versions.toml.
medousa_package_version() {
  local package_id="$1"
  local path line value
  path="$(medousa_package_versions_path)"
  if [[ ! -f "${path}" ]]; then
    echo "error: missing package versions file: ${path}" >&2
    return 1
  fi
  line="$(grep -E "^[[:space:]]*${package_id}[[:space:]]*=" "${path}" | head -1 || true)"
  if [[ -z "${line}" ]]; then
    echo "error: package version not found for '${package_id}' in ${path}" >&2
    return 1
  fi
  value="$(printf '%s' "${line}" | sed -n 's/.*=[[:space:]]*"\([^"]*\)".*/\1/p')"
  if [[ -z "${value}" ]]; then
    echo "error: could not parse version for '${package_id}' in ${path}" >&2
    return 1
  fi
  echo "${value}"
}

# Max of two dotted semver-ish strings (numeric segments only).
medousa_semver_max() {
  local a="$1" b="$2"
  local IFS=.
  # shellcheck disable=SC2206
  local -a aa=(${a}) bb=(${b})
  local i ai bi
  local len="${#aa[@]}"
  if [[ "${#bb[@]}" -gt "${len}" ]]; then
    len="${#bb[@]}"
  fi
  for ((i = 0; i < len; i++)); do
    ai="${aa[i]:-0}"
    bi="${bb[i]:-0}"
    if ((10#${ai} > 10#${bi})); then
      echo "${a}"
      return 0
    fi
    if ((10#${ai} < 10#${bi})); then
      echo "${b}"
      return 0
    fi
  done
  echo "${a}"
}

# Highest version among all package-versions.toml entries (channel-head candidate).
medousa_max_package_version() {
  local id v max=""
  for id in "${MEDOUSA_PACKAGE_VERSION_IDS[@]}"; do
    v="$(medousa_package_version "${id}")"
    if [[ -z "${max}" ]]; then
      max="${v}"
    else
      max="$(medousa_semver_max "${max}" "${v}")"
    fi
  done
  echo "${max}"
}

# When shipping WhatsApp, crate version must match adapter-whatsapp package stamp.
# Full-train releases additionally assert every package stamp equals the train version.
medousa_assert_whatsapp_package_version() {
  local wa_v pkg_v
  wa_v="$(medousa_whatsapp_version)"
  pkg_v="$(medousa_package_version adapter-whatsapp)"
  if [[ "${wa_v}" != "${pkg_v}" ]]; then
    echo "error: version mismatch — whatsapp Cargo.toml (${wa_v}) != package-versions adapter-whatsapp (${pkg_v})" >&2
    exit 1
  fi
}

# Backward-compatible name: only checks WhatsApp package stamp (no global lockstep).
medousa_assert_versions_match() {
  medousa_assert_whatsapp_package_version
}

# For `v*` full trains: every package-versions.toml entry must equal TRAIN_VERSION.
medousa_assert_full_train_versions() {
  local train="${1:-}"
  local id v
  if [[ -z "${train}" ]]; then
    echo "error: medousa_assert_full_train_versions requires a version" >&2
    exit 1
  fi
  for id in "${MEDOUSA_PACKAGE_VERSION_IDS[@]}"; do
    v="$(medousa_package_version "${id}")"
    if [[ "${v}" != "${train}" ]]; then
      echo "error: full-train release v${train} requires package-versions.toml ${id}=\"${train}\" (found \"${v}\")" >&2
      exit 1
    fi
  done
  medousa_assert_whatsapp_package_version
  local root_v
  root_v="$(medousa_version)"
  if [[ "${root_v}" != "${train}" ]]; then
    echo "error: full-train release v${train} requires root Cargo.toml version \"${train}\" (found \"${root_v}\")" >&2
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
# engine = launcher + daemon + CLI + TUI (headless core). No full-suite archive.
MEDOUSA_COMPONENT_IDS=(
  engine
  adapter-telegram
  adapter-discord
  adapter-slack
  adapter-whatsapp
  mcp-gateway
)

medousa_component_binaries() {
  case "$1" in
    engine) echo "medousa medousa_daemon medousa_cli medousa_tui" ;;
    # Legacy alias — redirected to engine contents for older scripts.
    cli) echo "medousa medousa_daemon medousa_cli medousa_tui" ;;
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
    engine) echo "core" ;;
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
    adapter-* | mcp-gateway | local-brain) echo "engine" ;;
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

# Archive name using package-versions.toml for the package id.
medousa_component_archive_for_package() {
  local package_id="$1"
  local target="$2"
  local version
  version="$(medousa_package_version "${package_id}")"
  medousa_component_archive_name "${package_id}" "${version}" "${target}"
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

medousa_default_cargo_target_dir() {
  local root
  root="$(medousa_repo_root)"
  echo "$(cd "${root}/.." && pwd)/.cache/cargo-target"
}

medousa_cargo_target_root() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "${CARGO_TARGET_DIR}"
  elif [[ -n "${MEDOUSA_CARGO_TARGET_DIR:-}" ]]; then
    echo "${MEDOUSA_CARGO_TARGET_DIR}"
  else
    medousa_default_cargo_target_dir
  fi
}

medousa_whatsapp_cargo_target_root() {
  medousa_cargo_target_root
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
      if medousa_is_optional_binary "${bin}"; then
        continue
      fi
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
  local bin file
  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    if [[ ! -f "${bin_dir}/${file}" ]] && medousa_is_optional_binary "${bin}"; then
      continue
    fi
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

# Tauri installer productName is "Medousa Installer" → "Medousa Installer_0.1.0_…" on disk.
# Legacy builds used "MedousaInstaller_*" (no space). Match both.
medousa_is_installer_bundle_name() {
  local name="$1"
  [[ "${name}" == MedousaInstaller* || "${name}" == "Medousa Installer"* ]]
}

medousa_find_release_file() {
  local dist_dir="$1"
  shift
  local pattern found
  for pattern in "$@"; do
    found="$(find "${dist_dir}" -maxdepth 3 -type f -name "${pattern}" 2>/dev/null | head -1 || true)"
    if [[ -n "${found}" ]]; then
      echo "${found}"
      return 0
    fi
  done
  return 1
}

medousa_find_installer_bundle() {
  local dist_dir="$1"
  local ext="$2"
  medousa_find_release_file "${dist_dir}" \
    "MedousaInstaller*.${ext}" \
    "Medousa Installer*.${ext}" || true
}

medousa_find_desktop_bundle() {
  local dist_dir="$1"
  local ext="$2"
  local found base
  while IFS= read -r found; do
    base="$(basename "${found}")"
    medousa_is_installer_bundle_name "${base}" && continue
    echo "${found}"
    return 0
  done < <(find "${dist_dir}" -maxdepth 3 -type f -name "Medousa_*.${ext}" 2>/dev/null || true)
  return 1
}

medousa_desktop_bundle_for_platform() {
  local dist_dir="$1"
  local platform="$2"
  case "${platform}" in
    macos-aarch64|macos-x64)
      medousa_find_desktop_bundle "${dist_dir}" dmg
      ;;
    windows-x64)
      medousa_find_desktop_bundle "${dist_dir}" exe \
        || medousa_find_desktop_bundle "${dist_dir}" msi
      ;;
    linux-x64)
      medousa_find_desktop_bundle "${dist_dir}" AppImage \
        || medousa_find_desktop_bundle "${dist_dir}" deb
      ;;
    *)
      return 1
      ;;
  esac
}

medousa_installer_bundle_for_platform() {
  local dist_dir="$1"
  local platform="$2"
  case "${platform}" in
    macos-aarch64|macos-x64)
      medousa_find_installer_bundle "${dist_dir}" dmg
      ;;
    windows-x64)
      medousa_find_installer_bundle "${dist_dir}" exe \
        || medousa_find_installer_bundle "${dist_dir}" msi
      ;;
    linux-x64)
      medousa_find_installer_bundle "${dist_dir}" AppImage \
        || medousa_find_installer_bundle "${dist_dir}" deb
      ;;
    *)
      return 1
      ;;
  esac
}

# Default download artifact per platform for installer-bootstrap.json.
# Windows express path ships the signed desktop NSIS setup (daemon sidecar bundled).
# Mac/Linux still use Medousa Installer for first-run component selection.
medousa_bootstrap_bundle_for_platform() {
  local dist_dir="$1"
  local platform="$2"
  case "${platform}" in
    macos-aarch64|macos-x64|linux-x64)
      medousa_installer_bundle_for_platform "${dist_dir}" "${platform}"
      ;;
    windows-x64)
      medousa_desktop_bundle_for_platform "${dist_dir}" "${platform}"
      ;;
    *)
      return 1
      ;;
  esac
}

medousa_assert_release_manifest_nonempty() {
  local manifest_path="$1"
  medousa_require_cmd jq
  local count
  count="$(jq '.packages | length' "${manifest_path}")"
  if [[ "${count}" -lt 1 ]]; then
    echo "error: ${manifest_path} has no packages indexed" >&2
    exit 1
  fi
}

medousa_assert_installer_bootstrap_nonempty() {
  local bootstrap_path="$1"
  medousa_require_cmd jq
  local count
  count="$(jq '.platforms | length' "${bootstrap_path}")"
  if [[ "${count}" -lt 1 ]]; then
    echo "error: ${bootstrap_path} has no platforms indexed" >&2
    exit 1
  fi
}
