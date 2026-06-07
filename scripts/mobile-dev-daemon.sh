#!/usr/bin/env bash
# Start medousa_daemon reachable from iPhone on the same Wi-Fi (bind 0.0.0.0).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIND="${MEDOUSA_MOBILE_BIND:-0.0.0.0:7419}"

cd "$ROOT"

LAN_IP=""
if command -v ipconfig >/dev/null 2>&1; then
  LAN_IP="$(ipconfig getifaddr en0 2>/dev/null || true)"
fi

echo "Starting medousa_daemon on ${BIND}"
if [[ -n "$LAN_IP" ]]; then
  echo "Point Medousa Home (iPhone) at: http://${LAN_IP}:7419"
else
  echo "Point Medousa Home at: http://<this-mac-lan-ip>:7419"
fi
echo ""

if command -v medousa >/dev/null 2>&1; then
  exec medousa start daemon --bind "$BIND"
fi

exec cargo run -p medousa --bin medousa_daemon -- --bind "$BIND"
