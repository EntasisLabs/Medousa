#!/usr/bin/env bash
# Validate Rust + Python SDK implementations against sdk-contract/manifest.yaml
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/sdk-contract/manifest.yaml"
RUST_SDK="$ROOT/crates/medousa-sdk/src"
PY_SDK="$ROOT/python/medousa-sdk/src/medousa"
errors=0

fail() { echo "ERROR: $*" >&2; ((errors++)) || true; }

if [[ ! -f "$MANIFEST" ]]; then
  fail "missing manifest: $MANIFEST"
  exit 1
fi

# Python: map accessor -> module file
py_module() {
  case "$1" in
    local_models) echo "local_models.py" ;;
    mcp_gateway) echo "mcp_gateway.py" ;;
    *) echo "$1.py" ;;
  esac
}

check_python_method() {
  local accessor="$1" method="$2" streaming="$3"
  local file="$PY_SDK/$(py_module "$accessor")"
  if [[ ! -f "$file" ]]; then
    fail "Python missing module for accessor '$accessor' ($file)"
    return
  fi
  if [[ "$streaming" == "true" ]]; then
    if ! grep -q "def ${method}\|async def ${method}" "$file" 2>/dev/null; then
      # stream_turn covers stream for interactive
      if [[ "$accessor" == "interactive" && "$method" == "stream" ]]; then
        grep -q "def stream\|stream_turn" "$file" || fail "Python interactive missing stream/stream_turn"
        return
      fi
      if [[ "$accessor" == "local_models" && "$method" == "download_events" ]]; then
        grep -q "download_events" "$file" || fail "Python local_models missing download_events"
        return
      fi
      if grep -q "status: planned" <<<"$(python3 - "$accessor" "$method" <<'PY'
import sys, yaml
from pathlib import Path
m = yaml.safe_load(Path(sys.argv[3] if len(sys.argv)>3 else "").read_text()) if False else None
PY
)" 2>/dev/null; then
        return
      fi
    fi
    grep -qE "def ${method}|async def ${method}" "$file" || fail "Python $accessor missing streaming method '$method' in $file"
  else
    if [[ "$accessor" == "interactive" && "$method" == "cancel" ]]; then
      grep -qE "def cancel|cancel_active_turn" "$file" || fail "Python interactive missing cancel in $file"
      return
    fi
    grep -qE "def ${method}|async def ${method}" "$file" || fail "Python $accessor missing method '$method' in $file"
  fi
}

check_rust_method() {
  local accessor="$1" method="$2" streaming="$3"
  local file
  case "$accessor" in
    local_models) file="$RUST_SDK/local.rs" ;;
    jobs) file="$RUST_SDK/jobs.rs" ;;
    recurring) file="$RUST_SDK/recurring.rs" ;;
    mcp_gateway) file="$RUST_SDK/mcp_gateway.rs" ;;
    health|ingest) file="$RUST_SDK/health.rs" ;;
    *) file="$RUST_SDK/${accessor}.rs" ;;
  esac
  if [[ ! -f "$file" ]]; then
    fail "Rust missing module for accessor '$accessor' ($file)"
    return
  fi
  if [[ "$streaming" == "true" ]]; then
    if [[ "$accessor" == "interactive" && "$method" == "stream" ]]; then
      grep -q "fn stream\|stream_turn" "$file" || fail "Rust interactive missing stream/stream_turn in $file"
      return
    fi
    if [[ "$accessor" == "local_models" && "$method" == "download_events" ]]; then
      grep -q "download_events" "$file" || fail "Rust local_models missing download_events in $file"
      return
    fi
    grep -q "fn ${method}" "$file" || fail "Rust $accessor missing streaming method '$method' in $file"
  else
    if [[ "$accessor" == "interactive" && "$method" == "cancel" ]]; then
      grep -qE "fn cancel|cancel_active_turn" "$file" || fail "Rust interactive missing cancel in $file"
      return
    fi
    if [[ "$accessor" == "sessions" && "$method" == "cancel_active_turn" ]]; then
      grep -q "cancel_active_turn" "$file" || fail "Rust sessions missing cancel_active_turn in $file"
      return
    fi
    grep -q "fn ${method}" "$file" || fail "Rust $accessor missing method '$method' in $file"
  fi
}

# Parse manifest with python (requires PyYAML — installed in python-sdk CI job)
parse_manifest() {
  local py="${PYTHON:-python3}"
  "$py" - "$MANIFEST" <<'PY'
import sys
try:
    import yaml
except ImportError:
    print("ERROR: PyYAML required (pip install pyyaml)", file=sys.stderr)
    sys.exit(2)
from pathlib import Path
data = yaml.safe_load(Path(sys.argv[1]).read_text())
methods = data.get("methods", [])
if len(methods) < 40:
    print(f"ERROR: manifest methods count too low ({len(methods)}); check YAML structure", file=sys.stderr)
    sys.exit(3)
helpers = data.get("client_helpers", [])
for entry in helpers:
    if isinstance(entry, dict) and "http" in entry:
        print(f"ERROR: client_helpers entry must not define HTTP routes: {entry.get('accessor')}", file=sys.stderr)
        sys.exit(4)
for entry in methods:
    if entry.get("status") == "planned":
        continue
    acc = entry["accessor"]
    meth = entry["method"]
    stream = str(entry.get("streaming", False)).lower()
    print(f"{acc}\t{meth}\t{stream}")
PY
}

if ! parse_manifest > /tmp/medousa-manifest-methods.tsv 2>&1; then
  parse_manifest 2>&1 || true
  fail "failed to parse sdk-contract/manifest.yaml (install PyYAML or fix manifest structure)"
  exit 1
fi

method_count=$(wc -l < /tmp/medousa-manifest-methods.tsv | tr -d ' ')
if [[ "$method_count" -lt 40 ]]; then
  fail "manifest parsed only $method_count methods (expected >= 40)"
  exit 1
fi

while IFS=$'\t' read -r accessor method streaming; do
  [[ -z "$accessor" ]] && continue
  check_python_method "$accessor" "$method" "$streaming"
  check_rust_method "$accessor" "$method" "$streaming"
done < /tmp/medousa-manifest-methods.tsv

# Manifest must exist for docs
if [[ ! -f "$ROOT/docs/sdk/python.md" ]]; then
  fail "docs/sdk/python.md missing"
fi

if [[ $errors -gt 0 ]]; then
  echo "check-sdk-contract: $errors issue(s)" >&2
  exit 1
fi
echo "check-sdk-contract: OK"
