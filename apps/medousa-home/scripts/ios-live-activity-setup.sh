#!/usr/bin/env bash
# Wire ios-live-activity Swift sources into the generated Xcode project.
# Run once after `npm run tauri:ios:init` (or whenever gen/apple is regenerated).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GEN="$ROOT/src-tauri/gen/apple"
SWIFT_SRC="$ROOT/src-tauri/ios-live-activity"
PROJECT_YML="$GEN/project.yml"

if [[ ! -f "$PROJECT_YML" ]]; then
  echo "[live-activity] gen/apple/project.yml not found — run: npm run tauri:ios:init"
  exit 1
fi

echo "[live-activity] Patching project.yml to include native bridge sources…"

# Add ios-live-activity App + Shared Swift to the iOS app target if not already present.
if ! grep -q "ios-live-activity/App" "$PROJECT_YML"; then
  PROJECT_YML="$PROJECT_YML" python3 - <<'PY'
from pathlib import Path
import os

project = Path(os.environ["PROJECT_YML"])
text = project.read_text()
needle = "      - path: medousa-home_iOS"
insert = """      - path: ../../ios-live-activity/App
      - path: ../../ios-live-activity/Shared"""
if needle in text and insert not in text:
    text = text.replace(needle, insert + "\n" + needle, 1)
    project.write_text(text)
    print("  added App + Shared Swift paths")
else:
    print("  App + Shared paths already present or anchor missing")
PY
fi

# Bump deployment target to 16.1 for Live Activities.
PROJECT_YML="$PROJECT_YML" python3 - <<'PY'
from pathlib import Path
import os
import re

project = Path(os.environ["PROJECT_YML"])
text = project.read_text()
text2 = re.sub(r"iOS: 15\.0", "iOS: 16.1", text)
if text2 != text:
    project.write_text(text2)
    print("  bumped iOS deployment target to 16.1")
PY

echo ""
echo "[live-activity] Next steps (one-time, in Xcode):"
echo "  1. open $GEN/*.xcodeproj"
echo "  2. File → New → Target → Widget Extension"
echo "     - Name: MedousaWorkWidget"
echo "     - Include Live Activity: YES"
echo "     - Include Configuration App Intent: NO"
echo "  3. Replace generated widget files with:"
echo "       src-tauri/ios-live-activity/Widget/MedousaWorkWidget.swift"
echo "       src-tauri/ios-live-activity/Widget/MedousaWorkWidgetBundle.swift"
echo "  4. Add Shared/MedousaWorkAttributes.swift to BOTH app + widget targets"
echo "  5. Signing & Capabilities → App Groups on both targets:"
echo "       group.com.entasislabs.medousa-home"
echo "  6. Regenerate project if using xcodegen: (cd $GEN && xcodegen)"
echo ""
echo "[live-activity] Done patching project.yml"
