#!/usr/bin/env bash
# Idempotent iOS project fixes after `tauri ios init` (entitlements, Live Activity, push).
# Called automatically from npm ios dev/build — do not run manually unless debugging.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GEN="$ROOT/src-tauri/gen/apple"
PROJECT_YML="$GEN/project.yml"

if [[ ! -f "$PROJECT_YML" ]]; then
  echo "[ios-prepare] skip: $PROJECT_YML not found (run: npm run tauri:ios:init)"
  exit 0
fi

ENT_SRC="$ROOT/src-tauri/ios-entitlements/medousa-home_iOS.entitlements"
ENT_DST="$GEN/medousa-home_iOS/medousa-home_iOS.entitlements"
if [[ -f "$ENT_SRC" && -d "$(dirname "$ENT_DST")" ]]; then
  if ! cmp -s "$ENT_SRC" "$ENT_DST" 2>/dev/null; then
    cp "$ENT_SRC" "$ENT_DST"
    echo "[ios-prepare] applied push entitlements"
  fi
fi

PROJECT_YML="$PROJECT_YML" python3 - <<'PY'
from pathlib import Path
import os
import re

project = Path(os.environ["PROJECT_YML"])
text = project.read_text()
original = text

needle = "      - path: medousa-home_iOS"
insert = """      - path: ../../ios-live-activity/App
      - path: ../../ios-live-activity/Shared"""
if needle in text and insert not in text:
    text = text.replace(needle, insert + "\n" + needle, 1)

text = re.sub(r"iOS: 15\.0", "iOS: 16.1", text)
text = re.sub(r"(deploymentTarget:\s*\n\s*iOS: )(\d+\.\d+)", lambda m: m.group(1) + ("16.1" if float(m.group(2)) < 16.1 else m.group(2)), text, count=1)

for framework in ("ActivityKit.framework", "WidgetKit.framework", "SwiftUI.framework"):
    sdk = f"      - sdk: {framework}"
    anchor = "      - sdk: WebKit.framework"
    if anchor in text and sdk not in text:
        text = text.replace(anchor, anchor + "\n" + sdk, 1)

if text != original:
    project.write_text(text)
    print("[ios-prepare] patched project.yml")
PY

if command -v xcodegen >/dev/null 2>&1; then
  (cd "$GEN" && xcodegen >/dev/null)
  echo "[ios-prepare] xcode project synced"
else
  echo "[ios-prepare] warn: install xcodegen (brew install xcodegen) to sync Xcode after patches"
fi
