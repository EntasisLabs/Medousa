#!/usr/bin/env bash
# wmi accepts windows + windows-core independently as >=0.59,<0.63. When both
# 0.61 (tauri/webview2) and 0.62 (iroh/netwatch) are in the graph, cargo can
# resolve wmi to a mismatched pair and the Windows desktop build fails with
# windows_result / IUnknownImpl errors. Keep the locked pair matched.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
failed=0

check_lock() {
  local lock="$1"
  python3 - "$lock" <<'PY'
import re, sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.is_file():
    print(f"skip missing {path}")
    raise SystemExit(0)

text = path.read_text()
failed = 0
for block in ("\n" + text).split("\n[[package]]\n"):
    if not re.search(r'^name = "wmi"$', block, re.M):
        continue
    deps = re.search(r"^dependencies = \[(.*?)\]", block, re.S | re.M)
    if not deps:
        print(f"FAIL {path}: wmi has no dependencies list", file=sys.stderr)
        failed = 1
        continue
    items = re.findall(r'"([^"]+)"', deps.group(1))
    windows = next((i for i in items if i.startswith("windows ")), None)
    core = next((i for i in items if i.startswith("windows-core ")), None)
    if not windows or not core:
        print(f"FAIL {path}: wmi missing windows/windows-core ({windows!r}, {core!r})", file=sys.stderr)
        failed = 1
        continue
    win_ver = windows.split()[1]
    core_ver = core.split()[1]
    win_mm = ".".join(win_ver.split(".")[:2])
    core_mm = ".".join(core_ver.split(".")[:2])
    if win_mm != core_mm:
        print(
            f"FAIL {path}: wmi resolved {windows} with {core} "
            f"(major.minor must match; restore a matched pair in the lockfile)",
            file=sys.stderr,
        )
        failed = 1
    else:
        print(f"ok {path}: wmi -> {windows}, {core}")
raise SystemExit(failed)
PY
}

while IFS= read -r -d '' lock; do
  if grep -q 'name = "wmi"' "$lock"; then
    if ! check_lock "$lock"; then
      failed=1
    fi
  fi
done < <(find "${ROOT}" -name Cargo.lock -not -path '*/target/*' -print0)

if [[ "${failed}" -ne 0 ]]; then
  exit 1
fi
