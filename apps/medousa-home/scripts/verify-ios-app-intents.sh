#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 /path/to/Medousa.app" >&2
  exit 2
fi

APP_BUNDLE="$1"
METADATA="$APP_BUNDLE/Metadata.appintents/extract.actionsdata"

if [[ ! -f "$METADATA" ]]; then
  echo "missing App Intents metadata: $METADATA" >&2
  exit 1
fi

METADATA="$METADATA" python3 - <<'PY'
import json
import os
from pathlib import Path

metadata_path = Path(os.environ["METADATA"])
metadata = json.loads(metadata_path.read_text())

intent_id = "MedousaIntegrationProbeIntent"
intent = metadata.get("actions", {}).get(intent_id)
if intent is None:
    raise SystemExit(f"missing intent: {intent_id}")
if not intent.get("isDiscoverable"):
    raise SystemExit(f"intent is not discoverable: {intent_id}")

shortcuts = metadata.get("autoShortcuts", [])
shortcut = next(
    (entry for entry in shortcuts if entry.get("actionIdentifier") == intent_id),
    None,
)
if shortcut is None:
    raise SystemExit(f"missing App Shortcut for intent: {intent_id}")

phrases = {
    phrase.get("key")
    for phrase in shortcut.get("phraseTemplates", [])
    if isinstance(phrase, dict)
}
required_phrases = {
    "Check ${applicationName}",
    "Check ${applicationName} integration",
}
missing_phrases = required_phrases - phrases
if missing_phrases:
    raise SystemExit(f"missing App Shortcut phrases: {sorted(missing_phrases)}")

print(
    f"verified {intent_id}: discoverable with {len(required_phrases)} shortcut phrases"
)
PY
