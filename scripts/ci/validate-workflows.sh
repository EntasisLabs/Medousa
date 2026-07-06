#!/usr/bin/env bash
# Quick static checks for GitHub Actions workflows (run before push).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
failed=0

while IFS= read -r -d '' file; do
  rel="${file#${ROOT}/}"
  if grep -E 'if:.*secrets\.' "$file" >/dev/null 2>&1; then
    echo "FAIL ${rel}: secrets context in if: (GitHub forbids this)" >&2
    grep -nE 'if:.*secrets\.' "$file" >&2 || true
    failed=1
  fi
  if grep -E 'secrets\.[A-Z0-9_]+\s*\|\|\s*secrets\.' "$file" >/dev/null 2>&1; then
    echo "FAIL ${rel}: secrets.A || secrets.B (use one direct secret reference)" >&2
    grep -nE 'secrets\.[A-Z0-9_]+\s*\|\|\s*secrets\.' "$file" >&2 || true
    failed=1
  fi
done < <(find "${ROOT}/.github/workflows" -name '*.yml' -o -name '*.yaml' -print0)

if [[ "${failed}" -eq 0 ]]; then
  echo "workflow checks passed"
else
  exit 1
fi
