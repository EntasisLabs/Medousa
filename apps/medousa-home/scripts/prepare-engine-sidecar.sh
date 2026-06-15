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

resolve_inference_features() {
  local mode="${MEDOUSA_EMBEDDED_INFERENCE:-auto}"
  case "${mode}" in
    metal)
      echo "--features embedded-inference-metal"
      return
      ;;
    cuda)
      echo "--features embedded-inference-cuda"
      return
      ;;
    cpu)
      echo "--features embedded-inference"
      return
      ;;
    auto)
      case "${TARGET}" in
        *-apple-*)
          echo "--features embedded-inference-metal"
          ;;
        *)
          # CPU builds work everywhere; set MEDOUSA_EMBEDDED_INFERENCE=cuda when building on a CUDA host.
          echo "--features embedded-inference"
          ;;
      esac
      return
      ;;
    *)
      echo "error: unknown MEDOUSA_EMBEDDED_INFERENCE=${mode} (expected auto|metal|cuda|cpu)" >&2
      exit 1
      ;;
  esac
}

FEATURES=()
case "$(resolve_inference_features)" in
  "--features embedded-inference-metal")
    FEATURES=(--features embedded-inference-metal)
    ;;
  "--features embedded-inference-cuda")
    FEATURES=(--features embedded-inference-cuda)
    ;;
  *)
    FEATURES=(--features embedded-inference)
    ;;
esac

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
