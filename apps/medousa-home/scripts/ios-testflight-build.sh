#!/usr/bin/env bash
# Build a TestFlight-ready IPA for Medousa Home (run on Mac from apps/medousa-home).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BUILD_NUMBER="${BUILD_NUMBER:-$(date +%s)}"
EXPORT_METHOD="${EXPORT_METHOD:-release-testing}"

echo "==> Medousa Home iOS release build"
echo "    version:     $(node -p "require('./src-tauri/tauri.conf.json').version")"
echo "    build:       $BUILD_NUMBER"
echo "    export:      $EXPORT_METHOD"
echo

npm run build
npm run tauri ios build -- --export-method "$EXPORT_METHOD" --build-number "$BUILD_NUMBER" --ci

IPA="$ROOT/src-tauri/gen/apple/build/Medousa Home.ipa"
if [[ -f "$IPA" ]]; then
  echo
  echo "[ok] IPA ready:"
  echo "     $IPA"
  echo
  echo "Next: open Transporter (Mac App Store) and upload this IPA to App Store Connect,"
  echo "      or run: open -a Transporter \"$IPA\""
else
  echo "[warn] Expected IPA not found at: $IPA"
  echo "       Check src-tauri/gen/apple/build/ for output."
fi
