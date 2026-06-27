#!/usr/bin/env bash
# Advisory doc consistency checks. Exit 0 unless --strict.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOCS="$ROOT/docs"
STRICT=0
if [[ "${1:-}" == "--strict" ]]; then
  STRICT=1
fi

errors=0
warn() { echo "WARN: $*" >&2; ((errors++)) || true; }
fail() { echo "ERROR: $*" >&2; ((errors++)) || true; }

# Banned stale route (daemon removed engine/load) — allow explicit "removed" / "not" mentions
while IFS= read -r -d '' f; do
  while IFS= read -r line; do
    if echo "$line" | grep -q '/v1/local/engine/load'; then
      if echo "$line" | grep -qiE 'removed|not `POST|not a daemon|probe-only'; then
        continue
      fi
      fail "stale route /v1/local/engine/load in $f: $line"
    fi
  done < "$f"
done < <(find "$DOCS" -name '*.md' -print0)

# docs/README.md must link SDK
if ! grep -q 'docs/sdk/README.md\|sdk/README.md' "$DOCS/README.md"; then
  fail "docs/README.md missing SDK link"
fi

# docs/README.md must link Python SDK
if ! grep -q 'sdk/python.md' "$DOCS/README.md"; then
  fail "docs/README.md missing Python SDK link"
fi

if [[ ! -f "$DOCS/sdk/python.md" ]]; then
  fail "docs/sdk/python.md missing"
fi

if [[ ! -f "$ROOT/sdk-contract/manifest.yaml" ]]; then
  fail "sdk-contract/manifest.yaml missing"
fi

# ADR index must list ADR-003
if ! grep -q 'adr-003' "$DOCS/architecture/decisions/README.md"; then
  fail "ADR index missing adr-003"
fi

# Resolve relative markdown links from docs/README.md (simple check)
while read -r link; do
  [[ "$link" =~ ^https?:// ]] && continue
  [[ "$link" =~ ^# ]] && continue
  target="${link%%#*}"
  [[ -z "$target" ]] && continue
  path="$DOCS/$target"
  if [[ ! -e "$path" ]]; then
    warn "broken link in README: $link -> $path"
  fi
done < <(grep -oE '\]\([^)]+\)' "$DOCS/README.md" | sed 's/](//;s/)$//')

if [[ $errors -gt 0 ]]; then
  echo "verify-docs: $errors issue(s)" >&2
  [[ $STRICT -eq 1 ]] && exit 1
  exit 0
fi

echo "verify-docs: OK"
exit 0
