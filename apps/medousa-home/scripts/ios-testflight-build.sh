#!/usr/bin/env bash
# Build a TestFlight-ready IPA for Medousa (run on Mac from apps/medousa-home).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BUILD_NUMBER="${BUILD_NUMBER:-$(date +%s)}"
EXPORT_METHOD="${EXPORT_METHOD:-release-testing}"

echo "==> Medousa iOS release build"
echo "    version:     $(node -p "require('./src-tauri/tauri.conf.json').version")"
echo "    build:       $BUILD_NUMBER"
echo "    export:      $EXPORT_METHOD"
echo

npm run build

npm run ios:prepare

npm run tauri:ios:build -- --export-method "$EXPORT_METHOD" --build-number "$BUILD_NUMBER" --ci

IPA="$(find "$ROOT/src-tauri/gen/apple/build" -name '*.ipa' -type f 2>/dev/null | head -1 || true)"
if [[ -n "$IPA" && -f "$IPA" ]]; then
  APP="$(find "$ROOT/src-tauri/gen/apple/build" -name '*.app' -type d 2>/dev/null | head -1 || true)"
  if [[ -n "$APP" && -f "$APP/libapp.a" ]]; then
    echo "[error] $APP contains libapp.a — run npm run ios:prepare and rebuild (libapp must be linked, not bundled)"
    exit 1
  fi
  if [[ -n "$APP" && -f "$APP/Info.plist" ]]; then
    MARKETING="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Info.plist" 2>/dev/null || echo '?')"
    BUNDLE="$(/usr/libexec/PlistBuddy -c 'Print CFBundleVersion' "$APP/Info.plist" 2>/dev/null || echo '?')"
    echo
    echo "[ok] Built app metadata:"
    echo "     CFBundleShortVersionString = $MARKETING"
    echo "     CFBundleVersion            = $BUNDLE"
  fi
  if [[ "${MEDOUSA_CARPLAY:-0}" == "1" && -n "$APP" ]]; then
    CARPLAY_ENTITLEMENT="com.apple.developer.carplay-voice-based-conversation"
    /usr/libexec/PlistBuddy -c \
      'Print :UIApplicationSceneManifest:UISceneConfigurations:CPTemplateApplicationSceneSessionRoleApplication' \
      "$APP/Info.plist" >/dev/null
    SIGNED_ENTITLEMENTS="$(codesign -d --entitlements - "$APP" 2>/dev/null)"
    if ! grep -q "$CARPLAY_ENTITLEMENT" <<<"$SIGNED_ENTITLEMENTS"; then
      echo "[error] CarPlay build is missing $CARPLAY_ENTITLEMENT in its signed entitlements"
      echo "        Request/enable CarPlay Voice Based Conversation for this App ID,"
      echo "        then regenerate the provisioning profile and rebuild."
      exit 1
    fi
    echo "[ok] Verified signed CarPlay scene + voice conversation entitlement"
  fi
  echo
  echo "[ok] IPA ready:"
  echo "     $IPA"
  echo
  echo "Next: open Transporter (Mac App Store) and upload this IPA to App Store Connect,"
  echo "      or run: open -a Transporter \"$IPA\""
else
  echo "[warn] No IPA found under src-tauri/gen/apple/build/"
  echo "       Check that export succeeded and look for *.ipa manually."
fi
