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

ask_intent_id = "AskMedousaIntent"
ask_intent = metadata.get("actions", {}).get(ask_intent_id)
if ask_intent is None or not ask_intent.get("isDiscoverable"):
    raise SystemExit(f"missing discoverable intent: {ask_intent_id}")
ask_parameters = {
    parameter.get("name") for parameter in ask_intent.get("parameters", [])
}
for parameter_name in ("prompt", "workshop"):
    if parameter_name not in ask_parameters:
        raise SystemExit(f"missing {parameter_name} parameter: {ask_intent_id}")
if not any("WorkshopEntity" in identifier for identifier in metadata.get("entities", {})):
    raise SystemExit("missing WorkshopEntity metadata")
if not any(
    entry.get("actionIdentifier") == ask_intent_id for entry in shortcuts
):
    raise SystemExit(f"missing App Shortcut for intent: {ask_intent_id}")
print(f"verified {ask_intent_id}: discoverable with prompt and workshop parameters")
PY
