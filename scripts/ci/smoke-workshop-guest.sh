#!/usr/bin/env bash
# Smoke the workshop guest without publishing to wasmer.io.
# Native cargo tests cover the HTTP contract. When the WASIX artifact and the
# Wasmer CLI are present, this also runs that wasm with Portal's --env flags.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"

echo "smoke-workshop-guest: cargo test -p medousa-workshop"
cargo test -p medousa-workshop

wasm="${root}/crates/medousa-workshop/dist/workshop.wasm"
if [[ ! -f "${wasm}" ]]; then
  if command -v cargo-wasix >/dev/null 2>&1; then
    bash "${root}/scripts/build-workshop-wasm.sh"
  else
    echo "smoke-workshop-guest: no dist/workshop.wasm and no cargo-wasix; native tests passed"
    exit 0
  fi
fi

if ! command -v wasmer >/dev/null 2>&1; then
  if [[ -f "${HOME}/.wasmer/wasmer.sh" ]]; then
    # shellcheck disable=SC1090
    source "${HOME}/.wasmer/wasmer.sh"
  fi
fi

if ! command -v wasmer >/dev/null 2>&1; then
  echo "smoke-workshop-guest: wasmer is not installed; native tests passed"
  exit 0
fi

port="$(python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
)"

MEDOUSA_SMOKE_PORT="${port}" python3 - <<'PY' &
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path != "/health":
            body = b'{"status":"no"}'
            status = 404
        elif self.headers.get("Authorization"):
            body = b'{"status":"rejected"}'
            status = 401
        else:
            body = b'{"status":"ok","apiVersion":"v1"}'
            status = 200
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format, *_args):
        return

port = int(os.environ["MEDOUSA_SMOKE_PORT"])
ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
server_pid=$!
trap 'kill "${server_pid}" >/dev/null 2>&1 || true' EXIT

ready=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  if python3 -c "import socket; s=socket.create_connection(('127.0.0.1', ${port}), 0.2); s.close()" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.1
done
if [[ "${ready}" != 1 ]]; then
  echo "smoke-workshop-guest: fake daemon did not listen on ${port}" >&2
  exit 1
fi

line="$(
  wasmer run --net --no-tty --offline \
    --env MEDOUSA_WORKSHOP_ONCE=1 \
    --env MEDOUSA_SANDBOX_ID=smoke \
    --env MEDOUSA_KIND=workshop \
    --env MEDOUSA_VCPU_MILLI=500 \
    --env MEDOUSA_MEMORY_MIB=256 \
    --env MEDOUSA_CONNECT_URL="http://127.0.0.1:${port}" \
    "${wasm}"
)"

python3 - "${line}" <<'PY'
import json, sys
ready = json.loads(sys.argv[1])
assert ready["event"] == "workshop.ready", ready
assert ready["sandboxId"] == "smoke"
assert ready["kind"] == "workshop"
assert ready["shape"]["vcpuMilli"] == 500
assert ready["shape"]["memoryMib"] == 256
assert ready["connect"]["ok"] is True, ready
assert ready["connect"]["mode"] == "http_health"
assert ready["connect"]["apiVersion"] == "v1"
assert ready["connect"]["heartbeat"] == "skipped"
assert ready["mesh"]["inGuest"] is False
assert ready["mesh"]["owner"] == "host_daemon"
print("smoke-workshop-guest: wasmer run ok")
PY
