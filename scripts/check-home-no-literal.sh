#!/usr/bin/env bash
# Home TS/Svelte must not embed daemon `/v1` route literals outside generated
# tables, tests, and the local browser-host bridge (not the daemon contract).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/apps/medousa-home"

hits="$(
  grep -R --include='*.ts' --include='*.svelte' -nE '"/v1/|'"'"'/v1/|`/v1/|\$\{base\}/v1/' src || true
)"
if [[ -z "$hits" ]]; then
  exit 0
fi

bad=0
while IFS= read -r line; do
  case "$line" in
    src/lib/daemon/generatedOps.ts:*)
      continue
      ;;
    src/lib/types/generated/*)
      continue
      ;;
    *.test.ts:*)
      continue
      ;;
    src/lib/browserBridge.ts:*)
      # Home-local browser host, not the workshop daemon contract.
      continue
      ;;
    *)
      echo "ERROR: Home helper still embeds /v1 literal: $line"
      bad=1
      ;;
  esac
done <<< "$hits"

exit "$bad"
