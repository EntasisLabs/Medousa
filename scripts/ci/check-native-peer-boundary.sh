#!/usr/bin/env bash
# Native is a daemon client, not a second peer networking authority.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
graph="$(cargo tree --locked --manifest-path "${ROOT}/apps/medousa-native/Cargo.toml" -e normal --prefix none)"
if grep -Eq '^(medousa-sdk-iroh|medousa-iroh-http|iroh|quinn|ed25519-dalek) v' <<<"${graph}"; then
  echo "native peer boundary violated: client depends on peer transport/signing" >&2
  exit 1
fi
features="$(cargo tree --locked --manifest-path "${ROOT}/apps/medousa-native/Cargo.toml" -e features --prefix none)"
if grep -Fq 'medousa-desktop-host feature "legacy-client-network"' <<<"${features}"; then
  echo "native peer boundary violated: legacy client networking enabled" >&2
  exit 1
fi
echo "check-native-peer-boundary: OK"
