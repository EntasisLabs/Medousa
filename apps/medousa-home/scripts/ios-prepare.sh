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

PROJECT_YML="$PROJECT_YML" TAURI_CONF="$ROOT/src-tauri/tauri.conf.json" python3 - <<'PY'
from pathlib import Path
import json
import os
import re

project = Path(os.environ["PROJECT_YML"])
tauri_conf = Path(os.environ["TAURI_CONF"])
text = project.read_text()
original = text

product_name = "Medousa"
development_team = os.environ.get("APPLE_DEVELOPMENT_TEAM", "").strip()
if tauri_conf.is_file():
    cfg = json.loads(tauri_conf.read_text())
    product_name = (cfg.get("productName") or product_name).strip()
    development_team = (
        development_team
        or (cfg.get("bundle") or {}).get("iOS", {}).get("developmentTeam", "")
        or ""
    ).strip()

# Live Activity Swift is compiled into libMedousaLiveActivity.a via build.rs — do NOT add
# ios-live-activity sources to the Xcode target (duplicate @_cdecl symbols crash at launch).
for stale in (
    "      - path: ../../ios-live-activity/App\n",
    "      - path: ../../ios-live-activity/Shared\n",
    "      - path: Externals\n",
):
    text = text.replace(stale, "")

text = re.sub(r"iOS: 15\.0", "iOS: 16.1", text)
text = re.sub(
    r"(deploymentTarget:\s*\n\s*iOS: )(\d+\.\d+)",
    lambda m: m.group(1) + ("16.1" if float(m.group(2)) < 16.1 else m.group(2)),
    text,
    count=1,
)

for framework in ("ActivityKit.framework", "WidgetKit.framework", "SwiftUI.framework"):
    text = text.replace(f"      - sdk: {framework}\n", "")

# FORCE_COLOR=0 is injected by Xcode/npm as a positional arg; tauri xcode-script
# mis-parses it as the arch (error: "0 isn't a known arch").
text = text.replace(
    "--configuration ${CONFIGURATION:?} ${FORCE_COLOR} ${ARCHS:?}",
    "--configuration ${CONFIGURATION:?} ${ARCHS:?}",
)

if product_name:
    text = re.sub(r"PRODUCT_NAME: .*", f"PRODUCT_NAME: {product_name}", text, count=1)

if development_team:
    if "DEVELOPMENT_TEAM:" in text:
        text = re.sub(r"DEVELOPMENT_TEAM: .*", f"DEVELOPMENT_TEAM: {development_team}", text)
    else:
        anchor = "      PRODUCT_BUNDLE_IDENTIFIER: com.entasislabs.medousa-home"
        insert = (
            f"{anchor}\n"
            f"      DEVELOPMENT_TEAM: {development_team}\n"
            f"      CODE_SIGN_STYLE: Automatic"
        )
        text = text.replace(anchor, insert, 1)
    if "CODE_SIGN_STYLE:" not in text:
        text = text.replace(
            f"DEVELOPMENT_TEAM: {development_team}",
            f"DEVELOPMENT_TEAM: {development_team}\n      CODE_SIGN_STYLE: Automatic",
            1,
        )
else:
    print("[ios-prepare] warn: no development team — set bundle.iOS.developmentTeam in tauri.conf.json or APPLE_DEVELOPMENT_TEAM")

strip_block = """    postBuildScripts:
      - script: |
          if [ -f "${TARGET_BUILD_DIR}/${WRAPPER_NAME}/libapp.a" ]; then
            rm -f "${TARGET_BUILD_DIR}/${WRAPPER_NAME}/libapp.a"
            echo "[ios-prepare] removed stray libapp.a from app bundle"
          fi
        name: Strip libapp.a from bundle
        basedOnDependencyAnalysis: false
"""
if "Strip libapp.a from bundle" not in text:
    anchor = "    preBuildScripts:"
    if anchor in text:
        text = text.replace(anchor, strip_block + anchor, 1)

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

# xcodegen can reintroduce libapp.a in Copy Bundle Resources — remove it.
PBXPROJ="$GEN/medousa-home.xcodeproj/project.pbxproj"
if [[ -f "$PBXPROJ" ]]; then
  if grep -q "libapp.a in Resources" "$PBXPROJ"; then
    python3 - <<'PY' "$PBXPROJ"
import re, sys
path = sys.argv[1]
text = open(path).read()
text = re.sub(r"\t\t[A-F0-9]+ /\* libapp\.a in Resources \*/ = \{isa = PBXBuildFile; fileRef = [A-F0-9]+ /\* libapp\.a \*/; \};\n", "", text)
text = re.sub(r"\t\t\t\t[A-F0-9]+ /\* libapp\.a in Resources \*/,\n", "", text)
open(path, "w").write(text)
PY
    echo "[ios-prepare] removed libapp.a from Xcode Resources phase"
  fi
fi
