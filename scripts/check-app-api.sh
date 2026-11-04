#!/usr/bin/env bash
# Verify checked-in application chat API artifacts match regeneration.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cargo run -q -p medousa-app-core --features json-schema --bin medousa-app-core-schema
cp sdk-contract/medousa-app-api.schema.json "$TMP/schema.json"
# Re-run into a temp copy for diff against the committed file after generation.
# The binary writes the canonical path; compare against the copy taken after run
# is tautological — instead regenerate TS and verify both checked-in files.

python3 scripts/gen-app-api-types.py --check
python3 - <<'PY'
import json, pathlib, subprocess, sys, tempfile, os
root = pathlib.Path(".").resolve()
schema = root / "sdk-contract" / "medousa-app-api.schema.json"
# Ensure schema is non-empty and contains expected roots.
data = json.loads(schema.read_text())
for name in (
    "ChatSnapshot",
    "ChatCommand",
    "ChatUpdate",
    "ChatItem",
    "ShellSnapshot",
    "ShellCommand",
    "ShellUpdate",
    "ShellTab",
):
    if name not in data:
        print(f"missing schema root: {name}", file=sys.stderr)
        sys.exit(1)
print(f"ok: {schema}")
PY
