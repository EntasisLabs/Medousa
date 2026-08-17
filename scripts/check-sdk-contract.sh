#!/usr/bin/env bash
# Compatibility wrapper: H10 contract gates live in check-api-contract.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec bash "$ROOT/scripts/check-api-contract.sh"
