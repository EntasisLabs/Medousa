#!/usr/bin/env bash
# Medousa enterprise installer — release archives, local dist, or in-repo source builds.
#
# Installs the full component set atomically (launcher + daemon + adapters) so sibling
# binary resolution never mixes stale builds. See scripts/release/common.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=release/common.sh
source "${SCRIPT_DIR}/release/common.sh"

MEDOUSA_VERSION="${MEDOUSA_VERSION:-latest}"
REGISTRY_URL="${MEDOUSA_RELEASE_BASE_URL:-}"
RELEASE_CHANNEL="${MEDOUSA_RELEASE_CHANNEL:-stable}"
INSTALL_DIR="${MEDOUSA_INSTALL_DIR:-${HOME}/.local/bin}"
STATE_DIR="${MEDOUSA_STATE_DIR:-${HOME}/.local/share/medousa}"
FROM_DIST=""
FROM_SOURCE=0
SYSTEM_INSTALL=0
DRY_RUN=0
FORCE=0
SKIP_CHECKSUM=0
VERIFY_ONLY=0
INSTALL_PROFILE=""

INSTALL_RECORD="${STATE_DIR}/install.json"

usage() {
  cat <<EOF
Usage: install.sh [options]

Install Medousa CLI components (launcher, daemon, TUI, adapters) as one versioned set.

Options:
  --version <tag>       Release tag (default: latest), e.g. v0.1.0
  --install-dir <dir>   Install directory (default: ~/.local/bin)
  --from-dist <path>    Install from a local release .tar.gz
  --from-source         Build from the current git checkout and install (recommended for dev)
  --system              Install to /usr/local/bin (requires write permission)
  --force               Install even if medousa_daemon is running
  --insecure-skip-checksum
                        Allow remote install without SHA256SUMS verification
  --verify-only         Validate the current install and exit
  --uninstall           Remove installed Medousa binaries from the install directory
  --registry-url <url>  Self-hosted release base URL (or MEDOUSA_RELEASE_BASE_URL)
  --channel <name>      Release channel (default: stable)
  --profile <name>      Install profile: default | headless-server (engine CLI only)
  -h, --help            Show this help

Environment:
  MEDOUSA_RELEASE_BASE_URL  Self-hosted artifact registry base URL
  MEDOUSA_RELEASE_CHANNEL     Release channel (default: stable)
  MEDOUSA_RELEASE_MANIFEST_URL  Override manifest URL
  MEDOUSA_GITHUB_REPO   GitHub repo fallback (owner/name, default: ${MEDOUSA_GITHUB_REPO})
  MEDOUSA_VERSION       Same as --version
  MEDOUSA_INSTALL_DIR   Same as --install-dir
  MEDOUSA_STATE_DIR     Install metadata directory (default: ~/.local/share/medousa)

Examples:
  # Production: pinned release from GitHub
  curl -fsSL https://raw.githubusercontent.com/${MEDOUSA_GITHUB_REPO}/main/scripts/install.sh | bash -s -- --version v0.1.0

  # Enterprise / air-gap: local artifact
  ./scripts/install.sh --from-dist dist/medousa-v0.1.0-x86_64-unknown-linux-gnu.tar.gz

  # Development: build and install from source (always coherent binary set)
  ./scripts/install.sh --from-source

  # Post-install validation
  ./scripts/install.sh --verify-only
EOF
}

log() {
  echo "[medousa-install] $*"
}

warn() {
  echo "[medousa-install] warning: $*" >&2
}

die() {
  echo "[medousa-install] error: $*" >&2
  exit 1
}

run() {
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    log "[dry-run] $*"
  else
    "$@"
  fi
}

require_sha_tool() {
  if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
    die "sha256sum or shasum is required"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      MEDOUSA_VERSION="$2"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --from-dist)
      FROM_DIST="$2"
      shift 2
      ;;
    --from-source)
      FROM_SOURCE=1
      shift
      ;;
    --system)
      SYSTEM_INSTALL=1
      INSTALL_DIR="/usr/local/bin"
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --insecure-skip-checksum)
      SKIP_CHECKSUM=1
      shift
      ;;
    --verify-only)
      VERIFY_ONLY=1
      shift
      ;;
    --uninstall)
      UNINSTALL=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --registry-url)
      REGISTRY_URL="$2"
      shift 2
      ;;
    --channel)
      RELEASE_CHANNEL="$2"
      shift 2
      ;;
    --profile)
      INSTALL_PROFILE="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (try --help)"
      ;;
  esac
done

if [[ "${INSTALL_PROFILE}" == "headless-server" ]]; then
  MEDOUSA_BINARIES=(medousa medousa_cli medousa_daemon medousa_local)
  log "profile=headless-server (engine binaries only — no TUI/adapters)"
elif [[ -n "${INSTALL_PROFILE}" && "${INSTALL_PROFILE}" != "default" ]]; then
  die "unknown profile: ${INSTALL_PROFILE} (try default or headless-server)"
fi

if [[ "${FROM_SOURCE}" -eq 1 && -n "${FROM_DIST}" ]]; then
  die "use either --from-source or --from-dist, not both"
fi

if [[ "${FROM_SOURCE}" -eq 1 && "${MEDOUSA_VERSION}" != "latest" ]]; then
  warn "--version is ignored with --from-source (version comes from Cargo.toml)"
fi

ensure_install_dir() {
  if [[ ! -d "${INSTALL_DIR}" ]]; then
    log "creating ${INSTALL_DIR}"
    run mkdir -p "${INSTALL_DIR}"
  fi
  if [[ ! -w "${INSTALL_DIR}" ]]; then
    die "install directory is not writable: ${INSTALL_DIR}"
  fi
}

path_contains_dir() {
  local dir="$1"
  case ":${PATH}:" in
    *":${dir}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

append_path_to_shell_rc() {
  local dir="$1"
  if path_contains_dir "${dir}"; then
    log "${dir} already on PATH"
    return 0
  fi

  local line="export PATH=\"${dir}:\$PATH\""
  local updated=0

  if [[ -n "${ZSH_VERSION:-}" || "${SHELL:-}" == *zsh* ]]; then
    local rc="${HOME}/.zshrc"
    if [[ -f "${rc}" ]] && grep -Fq "${dir}" "${rc}" 2>/dev/null; then
      log "PATH entry already present in ${rc}"
    else
      log "adding PATH to ${rc}"
      run bash -c "echo '' >> '${rc}' && echo '# Medousa' >> '${rc}' && echo '${line}' >> '${rc}'"
      updated=1
    fi
  fi

  if [[ -n "${BASH_VERSION:-}" || "${SHELL:-}" == *bash* ]]; then
    local rc="${HOME}/.bashrc"
    if [[ -f "${rc}" ]] && grep -Fq "${dir}" "${rc}" 2>/dev/null; then
      log "PATH entry already present in ${rc}"
    else
      log "adding PATH to ${rc}"
      run bash -c "echo '' >> '${rc}' && echo '# Medousa' >> '${rc}' && echo '${line}' >> '${rc}'"
      updated=1
    fi
  fi

  if [[ -d "${HOME}/.config/fish" ]]; then
    local fish_rc="${HOME}/.config/fish/config.fish"
    local fish_line="fish_add_path ${dir}"
    if [[ -f "${fish_rc}" ]] && grep -Fq "${dir}" "${fish_rc}" 2>/dev/null; then
      log "PATH entry already present in ${fish_rc}"
    else
      log "adding PATH to ${fish_rc}"
      run bash -c "echo '' >> '${fish_rc}' && echo '# Medousa' >> '${fish_rc}' && echo '${fish_line}' >> '${fish_rc}'"
      updated=1
    fi
  fi

  if [[ "${updated}" -eq 1 ]]; then
    log "open a new terminal or run: export PATH=\"${dir}:\$PATH\""
  elif ! path_contains_dir "${dir}"; then
    log "add to PATH manually: export PATH=\"${dir}:\$PATH\""
  fi
}

daemon_process_running() {
  if command -v pgrep >/dev/null 2>&1; then
    pgrep -x medousa_daemon >/dev/null 2>&1
    return $?
  fi
  return 1
}

preflight_install() {
  if daemon_process_running && [[ "${FORCE}" -eq 0 ]]; then
    die "medousa_daemon is running — stop it first or re-run with --force"
  fi

  if [[ "${SYSTEM_INSTALL}" -eq 1 && ! -w "${INSTALL_DIR}" ]]; then
    die "cannot write to ${INSTALL_DIR} — run with sudo or use default ~/.local/bin"
  fi

  ensure_install_dir
}

find_payload_root() {
  local extract_root="$1"
  if [[ -f "${extract_root}/install-manifest.json" && -d "${extract_root}/bin" ]]; then
    echo "${extract_root}"
    return 0
  fi
  local candidate
  candidate="$(find "${extract_root}" -maxdepth 2 -name install-manifest.json -print -quit 2>/dev/null || true)"
  if [[ -n "${candidate}" ]]; then
    dirname "${candidate}"
    return 0
  fi
  if [[ -d "${extract_root}/bin" ]]; then
    echo "${extract_root}"
    return 0
  fi
  candidate="$(find "${extract_root}" -type d -name bin | head -1)"
  [[ -n "${candidate}" ]] || die "could not locate bin/ or install-manifest.json in archive"
  dirname "${candidate}"
}

verify_payload() {
  local payload_root="$1"
  local target="$2"
  local bin_dir="${payload_root}/bin"
  local manifest="${payload_root}/install-manifest.json"

  [[ -d "${bin_dir}" ]] || die "missing bin/ in payload: ${payload_root}"

  local bin file src
  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    src="${bin_dir}/${file}"
    [[ -f "${src}" ]] || die "archive missing required binary: ${file}"
    if [[ ! -x "${src}" && "${DRY_RUN}" -eq 0 ]]; then
      run chmod +x "${src}"
    fi
  done

  if [[ -f "${manifest}" ]]; then
    local manifest_version manifest_target manifest_set_id actual_set_id
    manifest_version="$(medousa_read_manifest_field "${manifest}" version)"
    manifest_target="$(medousa_read_manifest_field "${manifest}" target)"
    manifest_set_id="$(medousa_read_manifest_field "${manifest}" component_set_id)"
    [[ -n "${manifest_version}" ]] || die "invalid install-manifest.json: missing version"
    [[ -n "${manifest_target}" ]] || die "invalid install-manifest.json: missing target"
    if [[ "${manifest_target}" != "${target}" ]]; then
      die "archive target ${manifest_target} does not match host ${target}"
    fi
    actual_set_id="$(medousa_component_set_id "${bin_dir}" "${target}")"
    if [[ "${manifest_set_id}" != "${actual_set_id}" ]]; then
      die "binary set fingerprint mismatch — archive may be corrupt or incomplete"
    fi
    log "verified manifest v${manifest_version} target=${manifest_target}"
    log "component_set_id=${manifest_set_id}"
  else
    warn "install-manifest.json not found — verifying binary presence only (legacy archive)"
    medousa_component_set_id "${bin_dir}" "${target}" >/dev/null
  fi
}

backup_existing_install() {
  local backup_root="${STATE_DIR}/install-backups"
  local stamp backup_dir bin file dst
  stamp="$(date -u +"%Y%m%dT%H%M%SZ")"
  backup_dir="${backup_root}/${stamp}"
  local found=0

  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "$(medousa_install_target_from_uname)")"
    dst="${INSTALL_DIR}/${file}"
    if [[ -f "${dst}" ]]; then
      if [[ "${found}" -eq 0 ]]; then
        log "backing up previous install to ${backup_dir}"
        run mkdir -p "${backup_dir}"
        found=1
      fi
      run cp -a "${dst}" "${backup_dir}/${file}"
    fi
  done

  if [[ -f "${INSTALL_RECORD}" ]]; then
    run mkdir -p "${backup_dir}"
    run cp -a "${INSTALL_RECORD}" "${backup_dir}/install.json"
  fi
}

install_payload() {
  local payload_root="$1"
  local target="$2"
  local version="$3"
  local source_label="$4"
  local bin_dir="${payload_root}/bin"
  local staging="${INSTALL_DIR}/.medousa-install-staging.$$"
  local bin file src dst

  verify_payload "${payload_root}" "${target}"
  backup_existing_install

  log "installing ${#MEDOUSA_BINARIES[@]} binaries to ${INSTALL_DIR}"
  run mkdir -p "${staging}"

  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    src="${bin_dir}/${file}"
    dst="${staging}/${file}"
    run cp -a "${src}" "${dst}"
    run chmod +x "${dst}" 2>/dev/null || true
  done

  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    src="${staging}/${file}"
    dst="${INSTALL_DIR}/${file}"
    run cp -f "${src}" "${dst}"
    run chmod +x "${dst}" 2>/dev/null || true
    log "  ${file}"
  done

  run rm -rf "${staging}"

  local component_set_id installed_at
  component_set_id="$(medousa_component_set_id "${INSTALL_DIR}" "${target}")"
  installed_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  run mkdir -p "${STATE_DIR}"
  if [[ "${DRY_RUN}" -eq 0 ]]; then
    cat >"${INSTALL_RECORD}" <<EOF
{
  "schema_version": 1,
  "product": "medousa",
  "version": "${version}",
  "target": "${target}",
  "install_dir": "${INSTALL_DIR}",
  "installed_at": "${installed_at}",
  "source": "${source_label}",
  "component_set_id": "${component_set_id}"
}
EOF
  else
    log "[dry-run] would write ${INSTALL_RECORD}"
  fi
}

postflight_install() {
  local target="$1"
  local launcher daemon_launcher
  launcher="$(medousa_binary_filename medousa "${target}")"
  daemon_launcher="$(medousa_binary_filename medousa_daemon "${target}")"

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    return 0
  fi

  local launcher_path="${INSTALL_DIR}/${launcher}"
  local daemon_path="${INSTALL_DIR}/${daemon_launcher}"

  [[ -x "${launcher_path}" ]] || die "post-install check failed: ${launcher_path} not executable"
  [[ -x "${daemon_path}" ]] || die "post-install check failed: ${daemon_path} not executable"

  if ! "${launcher_path}" --help >/dev/null 2>&1; then
    die "post-install check failed: ${launcher} --help"
  fi
  if ! "${daemon_path}" --help >/dev/null 2>&1; then
    die "post-install check failed: ${daemon_launcher} --help"
  fi

  local installed_set_id
  installed_set_id="$(medousa_component_set_id "${INSTALL_DIR}" "${target}")"
  if [[ -f "${INSTALL_RECORD}" ]]; then
    local recorded_set_id
    recorded_set_id="$(medousa_read_manifest_field "${INSTALL_RECORD}" component_set_id)"
    if [[ -n "${recorded_set_id}" && "${recorded_set_id}" != "${installed_set_id}" ]]; then
      die "post-install coherence check failed — binaries may be mismatched"
    fi
  fi

  log "post-install checks passed (launcher + daemon executable, set fingerprint OK)"
}

install_from_archive() {
  local archive_path="$1"
  local target="$2"
  local version="$3"
  local tmp payload_root

  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  log "extracting $(basename "${archive_path}")"
  run tar -xzf "${archive_path}" -C "${tmp}"
  payload_root="$(find_payload_root "${tmp}")"
  install_payload "${payload_root}" "${target}" "${version}" "archive:$(basename "${archive_path}")"
}

resolve_release_tag() {
  if [[ "${MEDOUSA_VERSION}" != "latest" ]]; then
    local tag="${MEDOUSA_VERSION}"
    [[ "${tag}" == v* ]] || tag="v${tag}"
    echo "${tag}"
    return 0
  fi

  if [[ -n "${REGISTRY_URL}" ]]; then
    medousa_require_cmd curl
    local manifest_url
    manifest_url="$(MEDOUSA_RELEASE_BASE_URL="${REGISTRY_URL}" MEDOUSA_RELEASE_CHANNEL="${RELEASE_CHANNEL}" medousa_release_manifest_url)"
    local version
    version="$(curl -fsSL "${manifest_url}" | sed -n 's/.*"version": "\([^"]*\)".*/\1/p' | head -1)"
    [[ -n "${version}" ]] || die "could not resolve version from ${manifest_url}"
    echo "v${version}"
    return 0
  fi

  medousa_require_cmd curl
  local tag
  tag="$(curl -fsSL "https://api.github.com/repos/${MEDOUSA_GITHUB_REPO}/releases/latest" \
    | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p' | head -1)"
  [[ -n "${tag}" ]] || die "could not resolve latest release tag from ${MEDOUSA_GITHUB_REPO}"
  echo "${tag}"
}

release_download_base() {
  local tag="$1"
  if [[ -n "${REGISTRY_URL}" ]]; then
    echo "${REGISTRY_URL%/}/${RELEASE_CHANNEL}"
    return 0
  fi
  echo "https://github.com/${MEDOUSA_GITHUB_REPO}/releases/download/${tag}"
}

download_release_file() {
  local tag="$1"
  local filename="$2"
  local out_path="$3"
  local base
  base="$(release_download_base "${tag}")"
  local url="${base}/${filename}"
  log "downloading ${filename}"
  run curl -fsSL -o "${out_path}" "${url}"
}

verify_archive_checksum() {
  local archive_path="$1"
  local checksums_path="$2"
  local archive_name
  archive_name="$(basename "${archive_path}")"

  if [[ ! -f "${checksums_path}" ]]; then
    if [[ "${SKIP_CHECKSUM}" -eq 1 ]]; then
      warn "SHA256SUMS missing — continuing (--insecure-skip-checksum)"
      return 0
    fi
    die "SHA256SUMS missing for remote install — use --insecure-skip-checksum to override"
  fi

  local expected actual
  expected="$(grep "  ${archive_name}$" "${checksums_path}" | awk '{print $1}' || true)"
  [[ -n "${expected}" ]] || die "no checksum entry for ${archive_name} in SHA256SUMS"
  actual="$(medousa_sha256_file "${archive_path}")"
  if [[ "${actual}" != "${expected}" ]]; then
    die "checksum mismatch for ${archive_name}"
  fi
  log "archive checksum verified"
}

install_from_release() {
  local target="$1"
  local tmp archive_path checksums_path version_tag version archive_name

  medousa_require_cmd curl
  require_sha_tool
  medousa_require_cmd tar

  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  version_tag="$(resolve_release_tag)"
  version="${version_tag#v}"
  archive_name="medousa-v${version}-${target}.tar.gz"
  archive_path="${tmp}/${archive_name}"
  checksums_path="${tmp}/SHA256SUMS"

  download_release_file "${version_tag}" "${archive_name}" "${archive_path}"
  if download_release_file "${version_tag}" "SHA256SUMS" "${checksums_path}" 2>/dev/null; then
    verify_archive_checksum "${archive_path}" "${checksums_path}"
  elif [[ "${SKIP_CHECKSUM}" -eq 1 ]]; then
    warn "SHA256SUMS not found on release — continuing (--insecure-skip-checksum)"
  else
    die "SHA256SUMS missing for remote install — use --insecure-skip-checksum to override"
  fi

  install_from_archive "${archive_path}" "${target}" "${version}"
}

install_from_source() {
  local target="${1}"
  local root staging version

  medousa_require_cmd cargo
  require_sha_tool

  root="$(medousa_repo_root)"
  [[ -f "${root}/Cargo.toml" ]] || die "--from-source requires a medousa git checkout"

  medousa_assert_versions_match
  version="$(medousa_version)"
  staging="${root}/dist/build/${target}"

  log "building medousa v${version} for ${target} from source"
  "${SCRIPT_DIR}/release/build.sh" --target "${target}" --output "${staging}"

  local payload_root="${staging}"
  if [[ ! -d "${staging}/bin" ]]; then
    die "build staging missing bin/: ${staging}/bin"
  fi

  medousa_write_install_manifest \
    "${staging}/bin" \
    "${version}" \
    "${target}" \
    "${staging}/install-manifest.json"

  install_payload "${staging}" "${target}" "${version}" "source:${root}"
}

verify_current_install() {
  local target
  target="$(medousa_install_target_from_uname)"

  if [[ ! -f "${INSTALL_RECORD}" ]]; then
    die "no install record at ${INSTALL_RECORD} — run install.sh first"
  fi

  local version install_target recorded_set_id actual_set_id
  version="$(medousa_read_manifest_field "${INSTALL_RECORD}" version)"
  install_target="$(medousa_read_manifest_field "${INSTALL_RECORD}" target)"
  recorded_set_id="$(medousa_read_manifest_field "${INSTALL_RECORD}" component_set_id)"

  [[ -n "${version}" ]] || die "install record missing version"
  [[ "${install_target}" == "${target}" ]] || warn "install target ${install_target} != host ${target}"

  actual_set_id="$(medousa_component_set_id "${INSTALL_DIR}" "${target}")"
  if [[ "${recorded_set_id}" != "${actual_set_id}" ]]; then
    die "install verification failed — binary set fingerprint mismatch (partial upgrade?)"
  fi

  log "install OK v${version} target=${install_target}"
  log "install_dir=${INSTALL_DIR}"
  log "component_set_id=${actual_set_id}"
  if command -v pgrep >/dev/null 2>&1 && pgrep -x medousa_daemon >/dev/null 2>&1; then
    log "medousa_daemon=running"
  else
    log "medousa_daemon=stopped"
  fi
}

uninstall_medousa() {
  local target bin file path removed=0
  target="$(medousa_install_target_from_uname)"

  if daemon_process_running && [[ "${FORCE}" -eq 0 ]]; then
    die "medousa_daemon is running — stop it first or use --force"
  fi

  for bin in "${MEDOUSA_BINARIES[@]}"; do
    file="$(medousa_binary_filename "${bin}" "${target}")"
    path="${INSTALL_DIR}/${file}"
    if [[ -f "${path}" ]]; then
      log "removing ${path}"
      run rm -f "${path}"
      removed=$((removed + 1))
    fi
  done

  if [[ -f "${INSTALL_RECORD}" ]]; then
    run rm -f "${INSTALL_RECORD}"
  fi

  if [[ "${removed}" -eq 0 ]]; then
    log "no Medousa binaries found in ${INSTALL_DIR}"
  else
    log "removed ${removed} binaries from ${INSTALL_DIR}"
  fi
}

main() {
  local target
  target="$(medousa_install_target_from_uname)"

  if [[ "${VERIFY_ONLY}" -eq 1 ]]; then
    verify_current_install
    exit 0
  fi

  if [[ "${UNINSTALL}" -eq 1 ]]; then
    uninstall_medousa
    exit 0
  fi

  preflight_install

  if [[ "${FROM_SOURCE}" -eq 1 ]]; then
    install_from_source "${target}"
  elif [[ -n "${FROM_DIST}" ]]; then
    [[ -f "${FROM_DIST}" ]] || die "archive not found: ${FROM_DIST}"
    require_sha_tool
    medousa_require_cmd tar
    local version="${MEDOUSA_VERSION}"
    if [[ "${version}" == "latest" ]]; then
      version="$(medousa_version_from_tag "$(basename "${FROM_DIST}" .tar.gz | sed -n 's/^medousa-v\([^-]*\)-.*/\1/p')")"
      [[ -n "${version}" ]] || version="unknown"
    else
      version="${version#v}"
    fi
    install_from_archive "${FROM_DIST}" "${target}" "${version}"
  else
    install_from_release "${target}"
  fi

  postflight_install "${target}"
  append_path_to_shell_rc "${INSTALL_DIR}"

  log ""
  log "Medousa v$(medousa_read_manifest_field "${INSTALL_RECORD}" version) installed to ${INSTALL_DIR}"
  log "Install record: ${INSTALL_RECORD}"
  log ""
  log "Next steps:"
  log "  1. Open a new terminal (or: export PATH=\"${INSTALL_DIR}:\$PATH\")"
  log "  2. medousa setup          # first-time configuration"
  log "  3. medousa doctor         # health check"
  log "  4. ./scripts/install.sh --verify-only   # validate binary set coherence"
  log ""
}

main "$@"
