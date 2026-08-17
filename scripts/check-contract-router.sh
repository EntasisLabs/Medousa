#!/usr/bin/env bash
# Production /v1 routes must be declared on DeclaredRouter / ContractRouter.
# Compatibility adapters listed below are reviewed exceptions until they import.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Axum string-literal /v1 registration. DeclaredRouter::route(policy, ...) does
# not match this pattern because the path lives on RoutePolicy.
hits="$(grep -R --include='*.rs' -nE '\.(route|nest)\("/v1' src || true)"
if [[ -z "$hits" ]]; then
  exit 0
fi

bad=0
while IFS= read -r line; do
  case "$line" in
    src/peer_scope.rs:*)
      # Test fixtures for the access boundary, not production assembly.
      continue
      ;;
    *)
      echo "ERROR: raw /v1 route registration outside ContractRouter: $line"
      bad=1
      ;;
  esac
done <<< "$hits"

exit "$bad"
