#!/usr/bin/env bash
# Copy medousa_daemon (slim) and optional medousa_local into Tauri sidecar binaries/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOME_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
MEDOUSA_ROOT="$(cd "${HOME_DIR}/../.." && pwd)"
BINARIES_DIR="${HOME_DIR}/src-tauri/binaries"

TARGET="${CARGO_BUILD_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
WITH_IROH=1
WITH_LOCAL_BRAIN=0

usage() {
  cat <<'EOF'
Usage: scripts/prepare-engine-sidecar.sh [options]

Options:
  --without-iroh       Omit iroh-transport (LAN-only pairing builds)
  --with-iroh          Include iroh-transport (default for Medousa.app)
  --with-local-brain   Also bundle medousa_local (Offline brain sidecar)
  -h, --help           Show this help

Environment:
  MEDOUSA_EMBEDDED_INFERENCE   auto|metal|cuda|cpu (for --with-local-brain only)
  MEDOUSA_WITH_IROH            0|false|no to omit iroh-transport
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --without-iroh)
      WITH_IROH=0
      shift
      ;;
    --with-iroh)
      WITH_IROH=1
      shift
      ;;
    --with-local-brain)
      WITH_LOCAL_BRAIN=1
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

case "${MEDOUSA_WITH_IROH:-}" in
  0 | false | FALSE | no | NO | off | OFF)
    WITH_IROH=0
    ;;
  1 | true | TRUE | yes | YES | on | ON)
    WITH_IROH=1
    ;;
esac

resolve_inference_feature() {
  local mode="${MEDOUSA_EMBEDDED_INFERENCE:-auto}"
  case "${mode}" in
    metal)
      echo "embedded-inference-metal"
      ;;
    cuda)
      echo "embedded-inference-cuda"
      ;;
    cpu)
      echo "embedded-inference"
      ;;
    auto)
      case "${TARGET}" in
        *-apple-*)
          echo "embedded-inference-metal"
          ;;
        *)
          echo "embedded-inference"
          ;;
      esac
      ;;
    *)
      echo "error: unknown MEDOUSA_EMBEDDED_INFERENCE=${mode} (expected auto|metal|cuda|cpu)" >&2
      exit 1
      ;;
  esac
}

mkdir -p "${BINARIES_DIR}"
TARGET_DIR="${CARGO_TARGET_DIR:-${MEDOUSA_CARGO_TARGET_DIR:-${MEDOUSA_ROOT}/../.cache/cargo-target}}"

find_release_binary() {
  local bin="$1"
  local file="${bin}"
  if [[ "${TARGET}" == *"-pc-windows-msvc" ]]; then
    file="${bin}.exe"
  fi
  local candidate="${TARGET_DIR}/${TARGET}/release/${file}"
  if [[ -f "${candidate}" ]]; then
    echo "${candidate}"
    return 0
  fi
  candidate="${TARGET_DIR}/release/${file}"
  if [[ -f "${candidate}" ]]; then
    echo "${candidate}"
    return 0
  fi
  candidate="${MEDOUSA_ROOT}/target/${TARGET}/release/${file}"
  if [[ -f "${candidate}" ]]; then
    echo "${candidate}"
    return 0
  fi
  candidate="${MEDOUSA_ROOT}/target/release/${file}"
  if [[ -f "${candidate}" ]]; then
    echo "${candidate}"
    return 0
  fi
  return 1
}

sidecar_name() {
  local base="$1"
  if [[ "${TARGET}" == *"-pc-windows-msvc" ]]; then
    echo "${base}-${TARGET}.exe"
  else
    echo "${base}-${TARGET}"
  fi
}

DAEMON_SIDECAR_NAME="$(sidecar_name medousa_daemon)"
LOCAL_SIDECAR_NAME="$(sidecar_name medousa_local)"

DAEMON_FEATURES=()
if [[ "${WITH_IROH}" -eq 1 ]]; then
  DAEMON_FEATURES+=("iroh-transport")
fi
if [[ ${#DAEMON_FEATURES[@]} -gt 0 ]]; then
  DAEMON_FEATURES_CSV="$(IFS=,; echo "${DAEMON_FEATURES[*]}")"
  DAEMON_FEATURE_ARGS=(--features "${DAEMON_FEATURES_CSV}")
else
  DAEMON_FEATURE_ARGS=()
fi

echo "prepare-engine-sidecar: building slim medousa_daemon for ${TARGET}…"
(
  cd "${MEDOUSA_ROOT}"
  cargo build --release -p medousa --bin medousa_daemon "${DAEMON_FEATURE_ARGS[@]}"
)

DAEMON_SRC="$(find_release_binary medousa_daemon)"
cp -f "${DAEMON_SRC}" "${BINARIES_DIR}/${DAEMON_SIDECAR_NAME}"
chmod +x "${BINARIES_DIR}/${DAEMON_SIDECAR_NAME}"
echo "prepare-engine-sidecar: ${BINARIES_DIR}/${DAEMON_SIDECAR_NAME}"

if [[ "${WITH_LOCAL_BRAIN}" -eq 1 ]]; then
  INFERENCE_FEATURE="$(resolve_inference_feature)"
  echo "prepare-engine-sidecar: building medousa_local (${INFERENCE_FEATURE})…"
  (
    cd "${MEDOUSA_ROOT}"
    cargo build --release -p medousa --bin medousa_local --features "${INFERENCE_FEATURE}"
  )
  LOCAL_SRC="$(find_release_binary medousa_local)"
  cp -f "${LOCAL_SRC}" "${BINARIES_DIR}/${LOCAL_SIDECAR_NAME}"
  chmod +x "${BINARIES_DIR}/${LOCAL_SIDECAR_NAME}"
  echo "prepare-engine-sidecar: ${BINARIES_DIR}/${LOCAL_SIDECAR_NAME}"
fi
