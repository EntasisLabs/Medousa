#!/usr/bin/env bash
# H10: IR tests, declared-inventory equality, no raw production /v1 Router.route,
# helper no-literal gates, and released-baseline semantic diff.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo test -p medousa-api-contract
cargo test -p medousa --lib daemon::contract::tests
cargo test -p medousa-sdk --test no_helper_route_literals
bash "$ROOT/scripts/check-contract-router.sh"
bash "$ROOT/scripts/check-home-no-literal.sh"

python_hits="$(
  grep -R --include='*.py' -nE '"/v1/|'"'"'/v1/' python/medousa-sdk/src/medousa || true
)"
if [[ -n "$python_hits" ]]; then
  bad=0
  while IFS= read -r line; do
    case "$line" in
      python/medousa-sdk/src/medousa/_generated/*)
        continue
        ;;
      *)
        echo "ERROR: Python helper still embeds /v1 literal: $line"
        bad=1
        ;;
    esac
  done <<< "$python_hits"
  exit "$bad"
fi
