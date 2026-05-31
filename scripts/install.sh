#!/usr/bin/env bash
# Install Medousa prebuilt binaries from GitHub Releases into ~/.local/bin.

set -euo pipefail

MEDOUSA_GITHUB_REPO="${MEDOUSA_GITHUB_REPO:-EntasisLabs/Medousa}"
MEDOUSA_VERSION="${MEDOUSA_VERSION:-latest}"
INSTALL_DIR="${MEDOUSA_INSTALL_DIR:-${HOME}/.local/bin}"
FROM_DIST=""
SYSTEM_INSTALL=0
DRY_RUN=0

usage() {
  cat <<EOF
Usage: install.sh [options]

Options:
  --version <tag>     Release tag (default: latest), e.g. v0.1.0
  --install-dir <dir> Install directory (default: ~/.local/bin)
  --from-dist <path>  Install from local .tar.gz (offline / smoke test)
  --system            Install to /usr/local/bin (requires write permission)
  --dry-run           Print actions without installing
  -h, --help          Show this help

Environment:
  MEDOUSA_GITHUB_REPO   GitHub repo (owner/name)
  MEDOUSA_VERSION       Same as --version

Examples:
  curl -fsSL https://raw.githubusercontent.com/${MEDOUSA_GITHUB_REPO}/main/scripts/install.sh | bash
  curl -fsSL .../install.sh | bash -s -- --version v0.1.0
  ./scripts/install.sh --from-dist dist/medousa-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
EOF
}

log() {
  echo "[medousa-install] $*"
}

die() {
  echo "[medousa-install] error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
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
    --system)
      SYSTEM_INSTALL=1
      INSTALL_DIR="/usr/local/bin"
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
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

# Mirror scripts/release/common.sh — keep in sync (see docs/internal/release-distribution.md).
install_target_from_uname() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Linux:x86_64) echo "x86_64-unknown-linux-gnu" ;;
    Linux:aarch64 | Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
    Darwin:arm64 | Darwin:aarch64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    MINGW*:* | MSYS*:* | CYGWIN*:*)
      die "Windows install.sh is not supported yet — download the .tar.gz from GitHub Releases or use WSL"
      ;;
    *) die "unsupported platform: ${os}/${arch}" ;;
  esac
}

binary_filename() {
  local name="$1"
  local target="$2"
  if [[ "${target}" == *"-pc-windows-msvc" ]]; then
    echo "${name}.exe"
  else
    echo "${name}"
  fi
}

run() {
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    log "[dry-run] $*"
  else
    "$@"
  fi
}

ensure_install_dir() {
  if [[ ! -d "${INSTALL_DIR}" ]]; then
    log "creating ${INSTALL_DIR}"
    run mkdir -p "${INSTALL_DIR}"
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

verify_sha256() {
  local archive_path="$1"
  local expected_hash="$2"
  local actual_hash
  actual_hash="$(sha256sum "${archive_path}" | awk '{print $1}')"
  if [[ "${actual_hash}" != "${expected_hash}" ]]; then
    die "checksum mismatch for $(basename "${archive_path}")"
  fi
  log "checksum verified"
}

install_from_archive() {
  local archive_path="$1"
  local target="$2"
  local tmp
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' EXIT

  log "extracting $(basename "${archive_path}")"
  run tar -xzf "${archive_path}" -C "${tmp}"

  local bin_src=""
  if [[ -d "${tmp}/bin" ]]; then
    bin_src="${tmp}/bin"
  else
    # Archive layout: medousa-vX.Y.Z-target/bin/
    bin_src="$(find "${tmp}" -type d -name bin | head -1)"
  fi
  [[ -n "${bin_src}" && -d "${bin_src}" ]] || die "could not find bin/ in archive"

  ensure_install_dir

  local bins=(
    medousa medousa_cli medousa_daemon medousa_tui
    medousa_telegram medousa_discord medousa_slack
    medousa_mcp_gateway medousa_whatsapp
  )

  for bin in "${bins[@]}"; do
    local file
    file="$(binary_filename "${bin}" "${target}")"
    local src="${bin_src}/${file}"
    local dst="${INSTALL_DIR}/${file}"
    [[ -f "${src}" ]] || die "missing ${file} in archive"
    log "installing ${file} → ${dst}"
    run cp -f "${src}" "${dst}"
    run chmod +x "${dst}" 2>/dev/null || true
  done
}

resolve_release_tag() {
  if [[ "${MEDOUSA_VERSION}" == "latest" ]]; then
    require_cmd curl
    local tag
    tag="$(curl -fsSL "https://api.github.com/repos/${MEDOUSA_GITHUB_REPO}/releases/latest" \
      | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p' | head -1)"
    [[ -n "${tag}" ]] || die "could not resolve latest release tag"
    echo "${tag}"
  else
    local tag="${MEDOUSA_VERSION}"
    [[ "${tag}" == v* ]] || tag="v${tag}"
    echo "${tag}"
  fi
}

download_release_file() {
  local tag="$1"
  local filename="$2"
  local out_path="$3"
  local url="https://github.com/${MEDOUSA_GITHUB_REPO}/releases/download/${tag}/${filename}"
  log "downloading ${filename}"
  run curl -fsSL -o "${out_path}" "${url}"
}

main() {
  require_cmd tar
  require_cmd sha256sum

  local target
  target="$(install_target_from_uname)"

  local tmp
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' EXIT

  local archive_path
  local version_tag

  if [[ -n "${FROM_DIST}" ]]; then
    [[ -f "${FROM_DIST}" ]] || die "archive not found: ${FROM_DIST}"
    archive_path="${FROM_DIST}"
    log "installing from local archive"
  else
    require_cmd curl
    local version_tag
    version_tag="$(resolve_release_tag)"
    local version="${version_tag#v}"

    local archive_name="medousa-v${version}-${target}.tar.gz"
    archive_path="${tmp}/${archive_name}"
    local checksums_path="${tmp}/SHA256SUMS"

    download_release_file "${version_tag}" "${archive_name}" "${archive_path}"
    if ! download_release_file "${version_tag}" "SHA256SUMS" "${checksums_path}" 2>/dev/null; then
      log "warning: SHA256SUMS not found on release — skipping verify"
    fi

    if [[ -f "${checksums_path}" ]]; then
      local expected
      expected="$(grep "  ${archive_name}$" "${checksums_path}" | awk '{print $1}' || true)"
      if [[ -n "${expected}" ]]; then
        verify_sha256 "${archive_path}" "${expected}"
      else
        log "warning: no checksum line for ${archive_name} in SHA256SUMS — skipping verify"
      fi
    fi
  fi

  if [[ "${SYSTEM_INSTALL}" -eq 1 ]] && [[ ! -w "${INSTALL_DIR}" ]]; then
    die "cannot write to ${INSTALL_DIR} — run with sudo or use default ~/.local/bin"
  fi

  install_from_archive "${archive_path}" "${target}"
  append_path_to_shell_rc "${INSTALL_DIR}"

  log ""
  log "Medousa installed to ${INSTALL_DIR}"
  log "Next: open a new terminal, then run:"
  log ""
  log "  medousa setup"
  log ""
}

main "$@"
