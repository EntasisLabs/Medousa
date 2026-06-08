#!/usr/bin/env bash
# Start medousa_daemon reachable from iPhone on the same Wi-Fi.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if command -v medousa >/dev/null 2>&1; then
  exec medousa start daemon --public
fi

exec cargo run -p medousa --bin medousa -- start daemon --public
