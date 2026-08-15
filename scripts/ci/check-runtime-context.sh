#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

# Tokio task-local compatibility shims are intentionally allowed until the
# upstream tool trait accepts an explicit invocation context. What must never
# return is process-global mutable request ownership or an optional respawn.
forbidden='set_active_tool_sink|with_optional_turn_execution_context|SNAPSHOT_TX|ACT_TX|NAV_STATE_TX|FIND_TX|RwLock<Option<TurnContinuationScope>>'

if rg --line-number --glob '*.rs' "${forbidden}" src apps/medousa-home/src-tauri/src; then
  echo "runtime-context guard: forbidden request-state compatibility symbol found" >&2
  exit 1
fi

echo "runtime-context guard: OK"
