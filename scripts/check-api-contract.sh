#!/usr/bin/env bash
# H10: IR tests, declared-inventory equality, and no raw production /v1 Router.route.
# sdk-contract/manifest.yaml remains a known-incomplete SDK accessor shadow until Slice 4.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo test -p medousa-api-contract
cargo test -p medousa --lib daemon::contract::tests
bash "$ROOT/scripts/check-contract-router.sh"
