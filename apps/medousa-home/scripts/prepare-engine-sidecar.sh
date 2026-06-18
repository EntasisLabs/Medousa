#!/usr/bin/env bash
# Copy medousa_daemon into Tauri sidecar binaries/ for bundling in Medousa.app

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOME_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Workspace root (Cargo.toml), not apps/
MEDOUSA_ROOT="$(cd "${HOME_DIR}/../.." && pwd)"
BINARIES_DIR="${HOME_DIR}/src-tauri/binaries"

TARGET="${CARGO_BUILD_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
SIDEcar_NAME="medousa_daemon-${TARGET}"
# Full-private desktop sidecar: Iroh + embedded inference by default.
WITH_IROH=1

usage() {
  cat <<'EOF'
Usage: scripts/prepare-engine-sidecar.sh [options]

Options:
  --without-iroh  Omit iroh-transport (LAN-only pairing builds)
  --with-iroh     Include iroh-transport (default for Medousa.app)
  -h, --help      Show this help

Environment:
  MEDOUSA_EMBEDDED_INFERENCE   auto|metal|cuda|cpu (default: auto — always on for sidecar)
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

CARGO_FEATURES=()
CARGO_FEATURES+=("$(resolve_inference_feature)")
if [[ "${WITH_IROH}" -eq 1 ]]; then
  CARGO_FEATURES+=("iroh-transport")
fi
FEATURES_CSV="$(IFS=,; echo "${CARGO_FEATURES[*]}")"
FEATURES=(--features "${FEATURES_CSV}")

mkdir -p "${BINARIES_DIR}"

echo "prepare-engine-sidecar: building medousa_daemon for ${TARGET} (${FEATURES[*]})…"
(
  cd "${MEDOUSA_ROOT}"
  cargo build --release -p medousa --bin medousa_daemon "${FEATURES[@]}"
)

TARGET_DIR="${CARGO_TARGET_DIR:-${MEDOUSA_ROOT}/target}"
SRC="${TARGET_DIR}/${TARGET}/release/medousa_daemon"
if [[ ! -f "${SRC}" ]]; then
  SRC="${TARGET_DIR}/release/medousa_daemon"
fi
if [[ ! -f "${SRC}" ]]; then
  echo "error: medousa_daemon not found after build (expected ${SRC})" >&2
  exit 1
fi

cp -f "${SRC}" "${BINARIES_DIR}/${SIDEcar_NAME}"
chmod +x "${BINARIES_DIR}/${SIDEcar_NAME}"
echo "prepare-engine-sidecar: ${BINARIES_DIR}/${SIDEcar_NAME}"
