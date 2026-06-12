#!/usr/bin/env bash
# Drop cached iOS build artifacts so the next build picks up fresh version/config.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Cleaning Medousa iOS build caches"

rm -rf "$ROOT/build" "$ROOT/.svelte-kit"
rm -rf "$ROOT/src-tauri/gen/apple/build"
rm -rf "$ROOT/src-tauri/target/aarch64-apple-ios" "$ROOT/src-tauri/target/aarch64-apple-ios-sim"

# Xcode DerivedData for this project (name varies after rebrand).
while IFS= read -r dir; do
  rm -rf "$dir"
done < <(find "$HOME/Library/Developer/Xcode/DerivedData" -maxdepth 1 -type d \( -name 'medousa-*' -o -name 'Medousa-*' \) 2>/dev/null || true)

echo "[ok] Frontend + Tauri iOS build dirs cleared."
echo
echo "If versions still look stale after rebuild, regenerate the Xcode project:"
echo "  rm -rf src-tauri/gen/apple && npm run tauri:ios:init"
