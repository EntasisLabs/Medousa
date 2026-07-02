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

apply_push_entitlements() {
  if [[ -f "$ENT_SRC" && -d "$(dirname "$ENT_DST")" ]]; then
    cp "$ENT_SRC" "$ENT_DST"
    echo "[ios-prepare] applied push entitlements"
  fi
}

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

text = re.sub(r"iOS: 16\.1", "iOS: 16.2", text)
text = re.sub(r"iOS: 15\.0", "iOS: 16.2", text)
text = re.sub(
    r"(deploymentTarget:\s*\n\s*iOS: )(\d+\.\d+)",
    lambda m: m.group(1) + ("16.2" if float(m.group(2)) < 16.2 else m.group(2)),
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

# Live Activity: enable Rust/Swift bridge during Xcode Rust build.
if "MEDOUSA_LIVE_ACTIVITY" not in text:
    text = text.replace(
        "        RUST_LOG: info\n",
        "        RUST_LOG: info\n        MEDOUSA_LIVE_ACTIVITY: \"1\"\n",
        1,
    )

# Live Activity: declare support in the main iOS app Info.plist (XcodeGen merge).
if "NSSSupportsLiveActivities" not in text:
    text = text.replace(
        "        CFBundleVersion: \"0.1.0\"\n",
        "        CFBundleVersion: \"0.1.0\"\n        NSSSupportsLiveActivities: true\n",
        1,
    )

# Widget Extension target for Lock Screen / Dynamic Island Live Activity UI.
if "MedousaWorkWidget:" not in text:
    widget_target = """
  MedousaWorkWidget:
    type: app-extension
    platform: iOS
    sources:
      - path: ../../ios-live-activity/Widget
        excludes:
          - Info.plist
          - MedousaWorkWidget.entitlements
      - path: ../../ios-live-activity/Shared
    info:
      path: ../../ios-live-activity/Widget/Info.plist
      properties:
        CFBundleDisplayName: Medousa Work
        NSExtension:
          NSExtensionPointIdentifier: com.apple.widgetkit-extension
    entitlements:
      path: ../../ios-live-activity/Widget/MedousaWorkWidget.entitlements
    settings:
      base:
        PRODUCT_NAME: MedousaWorkWidget
        PRODUCT_BUNDLE_IDENTIFIER: com.entasislabs.medousa-home.MedousaWorkWidget
        SKIP_INSTALL: YES
        GENERATE_INFOPLIST_FILE: NO
        LD_RUNPATH_SEARCH_PATHS: $(inherited) @executable_path/Frameworks @executable_path/../../Frameworks
      groups: [app]
"""
    text = text.rstrip() + widget_target

if "target: MedousaWorkWidget" not in text:
    text = text.replace(
        "      - sdk: WebKit.framework\n",
        "      - sdk: WebKit.framework\n      - target: MedousaWorkWidget\n        embed: true\n",
        1,
    )

# Xcode scheme env vars apply at Run time, not Build Rust Code — export for cargo/build.rs.
if "export MEDOUSA_LIVE_ACTIVITY=1" not in text:
    text = text.replace(
        "      - script: npm run -- tauri ios xcode-script",
        "      - script: |\n          export MEDOUSA_LIVE_ACTIVITY=1\n          npm run -- tauri ios xcode-script",
        1,
    )

# Widget extension must compile shared ActivityKit attribute types (older project.yml omits this).
if "MedousaWorkWidget:" in text and "../../ios-live-activity/Shared" not in text:
    text = text.replace(
        "      - path: ../../ios-live-activity/Widget\n        excludes:\n          - Info.plist\n          - MedousaWorkWidget.entitlements\n",
        "      - path: ../../ios-live-activity/Widget\n        excludes:\n          - Info.plist\n          - MedousaWorkWidget.entitlements\n      - path: ../../ios-live-activity/Shared\n",
        1,
    )

widget_entitlements_block = """    entitlements:
      path: ../../ios-live-activity/Widget/MedousaWorkWidget.entitlements
      properties:
        com.apple.security.application-groups:
          - group.com.entasislabs.medousa-home
"""
if "MedousaWorkWidget:" in text and "MedousaWorkWidget.entitlements\n      properties:" not in text:
    text = text.replace(
        "    entitlements:\n      path: ../../ios-live-activity/Widget/MedousaWorkWidget.entitlements\n",
        widget_entitlements_block,
        1,
    )

# XcodeGen regenerates entitlements plists on every run — without properties it writes an
# empty dict and strips aps-environment from the signed app (push + LA remote updates fail).
entitlements_block = """    entitlements:
      path: medousa-home_iOS/medousa-home_iOS.entitlements
      properties:
        aps-environment: development
        com.apple.security.application-groups:
          - group.com.entasislabs.medousa-home
"""
if "aps-environment" not in text:
    text = text.replace(
        "    entitlements:\n      path: medousa-home_iOS/medousa-home_iOS.entitlements\n",
        entitlements_block,
        1,
    )

if "com.apple.Push" not in text and "medousa-home_iOS:" in text:
    text = text.replace(
        "  medousa-home_iOS:\n    type: application\n",
        "  medousa-home_iOS:\n    type: application\n    attributes:\n      SystemCapabilities:\n        com.apple.Push:\n          enabled: 1\n",
        1,
    )

if text != original:
    project.write_text(text)
    print("[ios-prepare] patched project.yml")
PY

if command -v xcodegen >/dev/null 2>&1; then
  (cd "$GEN" && xcodegen >/dev/null)
  echo "[ios-prepare] xcode project synced"
  apply_push_entitlements
else
  echo "[ios-prepare] warn: install xcodegen (brew install xcodegen) to sync Xcode after patches"
  apply_push_entitlements
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
