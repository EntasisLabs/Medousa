#!/usr/bin/env bash
# Build selected Medousa release binaries for one Rust target into a staging directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
OUTPUT=""
PRINT_TARGET_ONLY=0
WITH_LOCAL_BRAIN=1
WITH_IROH=1
# Comma list: engine,adapters,mcp (default: all of them)
# `cli` is accepted as a legacy alias for engine.
COMPONENTS="engine,adapters,mcp"

usage() {
  cat <<'EOF'
Usage: scripts/release/build.sh [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --output <dir>        Staging directory (default: dist/build/<target>)
  --print-target        Print resolved target triple and exit
  --components <list>   Comma list: engine,adapters,mcp (default: all)
  --with-local-brain    Also build medousa_local into <output>/bin/ (default: on)
  --without-local-brain Skip medousa_local (mistralrs) build
  --without-iroh        Omit iroh-transport (LAN-only pairing)
  --with-iroh           Include iroh-transport (default)
  -h, --help            Show this help

Environment:
  MEDOUSA_PREBUILT_DAEMON   Path to a prebuilt medousa_daemon binary. When set,
                            engine packaging reuses it instead of compiling again.
  MEDOUSA_PREBUILT_LAUNCHER Path to a prebuilt medousa launcher (optional with daemon).

Component groups → bins:
  engine    medousa, medousa_daemon, medousa_cli, medousa_tui
  cli       legacy alias for engine
  adapters  medousa_telegram, medousa_discord, medousa_slack, medousa_whatsapp
  mcp       medousa_mcp_gateway
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --output)
      OUTPUT="$2"
      shift 2
      ;;
    --print-target)
      PRINT_TARGET_ONLY=1
      shift
      ;;
    --components)
      COMPONENTS="$2"
      shift 2
      ;;
    --with-local-brain)
      WITH_LOCAL_BRAIN=1
      shift
      ;;
    --without-local-brain)
      WITH_LOCAL_BRAIN=0
      shift
      ;;
    --without-iroh)
      WITH_IROH=0
      shift
      ;;
    --with-iroh)
      WITH_IROH=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

medousa_require_cmd cargo
medousa_require_cmd rustc

ROOT="$(medousa_repo_root)"
cd "${ROOT}"

if [[ -z "${TARGET}" ]]; then
  TARGET="$(medousa_host_target)"
fi

if [[ "${PRINT_TARGET_ONLY}" -eq 1 ]]; then
  echo "${TARGET}"
  exit 0
fi

if [[ -z "${OUTPUT}" ]]; then
  OUTPUT="${ROOT}/dist/build/${TARGET}"
fi

BIN_DIR="${OUTPUT}/bin"
mkdir -p "${BIN_DIR}"

want_component() {
  local needle="$1"
  local IFS=','
  local c
  for c in ${COMPONENTS}; do
    c="$(echo "${c}" | xargs)"
    [[ "${c}" == "${needle}" || "${c}" == "all" ]] && return 0
  done
  return 1
}

NEED_ENGINE=0
NEED_ADAPTERS=0
NEED_MCP=0
NEED_WHATSAPP=0
NEED_TELEGRAM=0
NEED_DISCORD=0
NEED_SLACK=0
# engine includes launcher + daemon + cli + tui (cli is a legacy alias for engine).
want_component engine && NEED_ENGINE=1
want_component cli && NEED_ENGINE=1
want_component adapters && NEED_ADAPTERS=1 && NEED_WHATSAPP=1 && NEED_TELEGRAM=1 && NEED_DISCORD=1 && NEED_SLACK=1
want_component mcp && NEED_MCP=1

if [[ "${NEED_WHATSAPP}" -eq 1 ]]; then
  medousa_assert_whatsapp_package_version
fi

VERSION="$(medousa_version)"
medousa_log "building medousa crate v${VERSION} for ${TARGET} (components: ${COMPONENTS})"
medousa_log "staging → ${BIN_DIR}"

CARGO_BUILD_ARGS=(--release)
CARGO_FEATURES=()
if [[ "${WITH_IROH}" -eq 1 ]]; then
  CARGO_FEATURES+=("iroh-transport")
  medousa_log "iroh transport enabled (default — runtime opt-out: MEDOUSA_IROH=0)"
fi
if [[ ${#CARGO_FEATURES[@]} -gt 0 ]]; then
  FEATURES_CSV="$(IFS=,; echo "${CARGO_FEATURES[*]}")"
  CARGO_BUILD_ARGS+=(--features "${FEATURES_CSV}")
fi
if [[ -n "${TARGET}" ]]; then
  CARGO_BUILD_ARGS+=(--target "${TARGET}")
fi

ROOT_BINS=()
if [[ "${NEED_ENGINE}" -eq 1 ]]; then
  if [[ -n "${MEDOUSA_PREBUILT_LAUNCHER:-}" && -f "${MEDOUSA_PREBUILT_LAUNCHER}" ]]; then
    medousa_log "reusing prebuilt launcher: ${MEDOUSA_PREBUILT_LAUNCHER}"
  else
    ROOT_BINS+=(medousa)
  fi
  if [[ -n "${MEDOUSA_PREBUILT_DAEMON:-}" && -f "${MEDOUSA_PREBUILT_DAEMON}" ]]; then
    medousa_log "reusing prebuilt daemon: ${MEDOUSA_PREBUILT_DAEMON}"
  else
    ROOT_BINS+=(medousa_daemon)
  fi
  ROOT_BINS+=(medousa_cli medousa_tui)
fi

if [[ ${#ROOT_BINS[@]} -gt 0 ]]; then
  medousa_log "cargo build (root): ${ROOT_BINS[*]}"
  BIN_ARGS=()
  for bin in "${ROOT_BINS[@]}"; do
    BIN_ARGS+=(--bin "${bin}")
  done
  cargo build "${CARGO_BUILD_ARGS[@]}" "${BIN_ARGS[@]}"
fi

build_adapter_manifest() {
  local manifest="$1"
  local label="$2"
  medousa_log "cargo build (${label})…"
  local ADAPTER_BUILD_ARGS=(--release --manifest-path "${manifest}")
  if [[ -n "${TARGET}" ]]; then
    ADAPTER_BUILD_ARGS+=(--target "${TARGET}")
  fi
  cargo build "${ADAPTER_BUILD_ARGS[@]}"
}

[[ "${NEED_TELEGRAM}" -eq 1 ]] && build_adapter_manifest "${MEDOUSA_TELEGRAM_MANIFEST}" "medousa_telegram"
[[ "${NEED_DISCORD}" -eq 1 ]] && build_adapter_manifest "${MEDOUSA_DISCORD_MANIFEST}" "medousa_discord"
[[ "${NEED_SLACK}" -eq 1 ]] && build_adapter_manifest "${MEDOUSA_SLACK_MANIFEST}" "medousa_slack"
[[ "${NEED_WHATSAPP}" -eq 1 ]] && build_adapter_manifest "${MEDOUSA_WHATSAPP_MANIFEST}" "medousa_whatsapp"
[[ "${NEED_MCP}" -eq 1 ]] && build_adapter_manifest "${MEDOUSA_MCP_GATEWAY_MANIFEST}" "medousa_mcp_gateway"

MAIN_RELEASE="$(medousa_cargo_release_dir "${TARGET}")"
WA_RELEASE="$(medousa_whatsapp_cargo_release_dir "${TARGET}")"

stage_bin() {
  local bin="$1"
  local src="$2"
  local dst="${BIN_DIR}/$(medousa_binary_filename "${bin}" "${TARGET}")"
  cp -f "${src}" "${dst}"
  chmod +x "${dst}" 2>/dev/null || true
  medousa_log "  ${bin}"
}

medousa_log "staging release binaries → ${BIN_DIR}"

if [[ "${NEED_ENGINE}" -eq 1 ]]; then
  if [[ -n "${MEDOUSA_PREBUILT_DAEMON:-}" && -f "${MEDOUSA_PREBUILT_DAEMON}" ]]; then
    stage_bin medousa_daemon "${MEDOUSA_PREBUILT_DAEMON}"
  else
    src="$(medousa_find_release_binary medousa_daemon "${TARGET}" || true)"
    [[ -n "${src}" ]] || {
      echo "error: expected binary missing: medousa_daemon" >&2
      exit 1
    }
    stage_bin medousa_daemon "${src}"
  fi
  if [[ -n "${MEDOUSA_PREBUILT_LAUNCHER:-}" && -f "${MEDOUSA_PREBUILT_LAUNCHER}" ]]; then
    stage_bin medousa "${MEDOUSA_PREBUILT_LAUNCHER}"
  else
    src="$(medousa_find_release_binary medousa "${TARGET}" || true)"
    [[ -n "${src}" ]] || {
      echo "error: expected binary missing: medousa" >&2
      exit 1
    }
    stage_bin medousa "${src}"
  fi
fi

STAGE_LIST=()
[[ "${NEED_ENGINE}" -eq 1 ]] && STAGE_LIST+=(medousa_cli medousa_tui)
[[ "${NEED_ADAPTERS}" -eq 1 ]] && STAGE_LIST+=(medousa_telegram medousa_discord medousa_slack medousa_whatsapp)
[[ "${NEED_MCP}" -eq 1 ]] && STAGE_LIST+=(medousa_mcp_gateway)

for bin in "${STAGE_LIST[@]}"; do
  src="$(medousa_find_release_binary "${bin}" "${TARGET}" || true)"
  if [[ -z "${src}" || ! -f "${src}" ]]; then
    echo "error: expected binary missing: ${bin} (searched under ${MAIN_RELEASE} and ${WA_RELEASE})" >&2
    exit 1
  fi
  stage_bin "${bin}" "${src}"
done

cat >"${OUTPUT}/build-meta.env" <<EOF
MEDOUSA_VERSION=${VERSION}
MEDOUSA_TARGET=${TARGET}
MEDOUSA_BIN_DIR=${BIN_DIR}
MEDOUSA_WITH_IROH=${WITH_IROH}
MEDOUSA_WITH_LOCAL_BRAIN=${WITH_LOCAL_BRAIN}
MEDOUSA_COMPONENTS=${COMPONENTS}
EOF

medousa_log "component build complete — $(find "${BIN_DIR}" -type f | wc -l | tr -d ' ') binaries in ${BIN_DIR}"

if [[ "${WITH_LOCAL_BRAIN}" -eq 1 ]]; then
  BRAIN_STAGING="${ROOT}/dist/build-local-brain/${TARGET}"
  medousa_log "building medousa_local offline brain (mistralrs)…"
  "${SCRIPT_DIR}/build-local-brain.sh" --target "${TARGET}" --output "${BRAIN_STAGING}"
  BRAIN_SRC="${BRAIN_STAGING}/bin/$(medousa_binary_filename medousa_local "${TARGET}")"
  if [[ ! -f "${BRAIN_SRC}" ]]; then
    echo "error: medousa_local missing after build-local-brain: ${BRAIN_SRC}" >&2
    exit 1
  fi
  cp -f "${BRAIN_SRC}" "${BIN_DIR}/$(medousa_binary_filename medousa_local "${TARGET}")"
  chmod +x "${BIN_DIR}/$(medousa_binary_filename medousa_local "${TARGET}")" 2>/dev/null || true
  medousa_log "  medousa_local → ${BIN_DIR}"
  "${SCRIPT_DIR}/package-local-brain.sh" --target "${TARGET}" --input "${BRAIN_STAGING}"
else
  medousa_log "skipping medousa_local — pass --with-local-brain or omit --without-local-brain"
fi

medousa_log "done — $(find "${BIN_DIR}" -type f | wc -l | tr -d ' ') binaries in ${BIN_DIR}"
