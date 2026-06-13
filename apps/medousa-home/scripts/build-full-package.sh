#!/usr/bin/env bash
# Desktop .app/.dmg + optional iOS TestFlight IPA (Mac only).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT}"

BUILD_IOS=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ios)
      BUILD_IOS=1
      shift
      ;;
    -h | --help)
      cat <<'EOF'
Usage: scripts/build-full-package.sh [--ios]

Builds:
  1. medousa_daemon sidecar (embedded inference on Apple Silicon)
  2. Medousa desktop app (tauri build)

With --ios (Mac + Xcode + paid Apple Developer for TestFlight):
  3. Medousa iOS IPA (release-testing export)

Desktop artifacts:
  src-tauri/target/release/bundle/

iOS IPA (when --ios):
  src-tauri/gen/apple/build/*.ipa
EOF
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

echo "==> Medousa full package build"
echo "    app dir: ${ROOT}"
echo

echo "==> 1/2 Engine sidecar + desktop app"
npm run tauri:build

echo
echo "[ok] Desktop bundle ready under:"
find "${ROOT}/src-tauri/target/release/bundle" -maxdepth 3 -type d -name "*.app" 2>/dev/null | head -5 || true

if [[ "${BUILD_IOS}" -eq 1 ]]; then
  echo
  echo "==> 2/2 iOS TestFlight IPA"
  bash "${SCRIPT_DIR}/ios-testflight-build.sh"
fi

echo
echo "[done] Full package build complete."
