#!/usr/bin/env bash
# Build all Medousa release binaries for one Rust target into a staging directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TARGET=""
OUTPUT=""
PRINT_TARGET_ONLY=0
WITH_LOCAL_BRAIN=1
# Full-private defaults: Iroh on unless explicitly disabled.
WITH_IROH=1

usage() {
  cat <<'EOF'
Usage: scripts/release/build.sh [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --output <dir>        Staging directory (default: dist/build/<target>)
  --print-target        Print resolved target triple and exit
  --with-local-brain    Also build medousa_local into <output>/bin/ (default: on)
  --without-local-brain Skip medousa_local (mistralrs) build
  --without-iroh        Omit iroh-transport (LAN-only pairing)
  --with-iroh           Include iroh-transport (default)
  -h, --help            Show this help

Builds all release binaries into <output>/bin/:
  medousa, medousa_cli, medousa_daemon, medousa_tui, channel adapters, medousa_mcp_gateway, medousa_whatsapp

By default also builds medousa_local (offline brain) into the same <output>/bin/ and packages
a separate medousa_local-*.tar.gz. Use --without-local-brain to skip the slow mistralrs build.

Iroh gateway is on at runtime when built with iroh-transport (opt out with MEDOUSA_IROH=0).
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

medousa_assert_versions_match
VERSION="$(medousa_version)"

medousa_log "building medousa v${VERSION} for ${TARGET}"
medousa_log "staging → ${BIN_DIR}"

medousa_log "phase 1/2: building CLI + daemon + channels (${#MEDOUSA_BINARIES[@]} binaries)…"
medousa_log "  bins: ${MEDOUSA_BINARIES[*]}"

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

medousa_log "phase 1/2: cargo build (root workspace, release)…"
cargo build "${CARGO_BUILD_ARGS[@]}" \
  --bin medousa \
  --bin medousa_cli \
  --bin medousa_daemon \
  --bin medousa_tui \
  --bin medousa_telegram \
  --bin medousa_discord \
  --bin medousa_slack \
  --bin medousa_mcp_gateway

medousa_log "cargo build (medousa_whatsapp)…"
WA_BUILD_ARGS=(--release --manifest-path "${MEDOUSA_WHATSAPP_MANIFEST}")
if [[ -n "${TARGET}" ]]; then
  WA_BUILD_ARGS+=(--target "${TARGET}")
fi
cargo build "${WA_BUILD_ARGS[@]}"

MAIN_RELEASE="$(medousa_cargo_release_dir "${TARGET}")"
WA_RELEASE="$(medousa_whatsapp_cargo_release_dir "${TARGET}")"

medousa_log "phase 1/2: staging release binaries → ${BIN_DIR}"
for bin in "${MEDOUSA_BINARIES[@]}"; do
  src=""
  if [[ "${bin}" == "medousa_whatsapp" ]]; then
    src="$(medousa_find_release_binary "${bin}" "${TARGET}" || true)"
  else
    src="$(medousa_find_release_binary "${bin}" "${TARGET}" || true)"
  fi
  if [[ -z "${src}" || ! -f "${src}" ]]; then
    echo "error: expected binary missing: ${bin} (searched under ${MAIN_RELEASE} and ${WA_RELEASE})" >&2
    exit 1
  fi
  dst="${BIN_DIR}/$(medousa_binary_filename "${bin}" "${TARGET}")"
  cp -f "${src}" "${dst}"
  chmod +x "${dst}" 2>/dev/null || true
  medousa_log "  ${bin}"
done

# Metadata for package step
cat >"${OUTPUT}/build-meta.env" <<EOF
MEDOUSA_VERSION=${VERSION}
MEDOUSA_TARGET=${TARGET}
MEDOUSA_BIN_DIR=${BIN_DIR}
MEDOUSA_WITH_IROH=${WITH_IROH}
MEDOUSA_WITH_LOCAL_BRAIN=${WITH_LOCAL_BRAIN}
EOF

medousa_log "phase 1/2 complete — $(find "${BIN_DIR}" -type f | wc -l | tr -d ' ') binaries in ${BIN_DIR}"

if [[ "${WITH_LOCAL_BRAIN}" -eq 1 ]]; then
  BRAIN_STAGING="${ROOT}/dist/build-local-brain/${TARGET}"
  medousa_log "phase 2/2: building medousa_local offline brain (mistralrs — slow, separate from daemon)…"
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
  medousa_log "phase 2/2 complete — medousa_local in ${BIN_DIR} + separate brain tarball in dist/"
else
  medousa_log "skipping phase 2 (medousa_local) — pass --with-local-brain or omit --without-local-brain"
fi

medousa_log "done — $(find "${BIN_DIR}" -type f | wc -l | tr -d ' ') binaries in ${BIN_DIR}"
