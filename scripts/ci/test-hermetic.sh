#!/usr/bin/env bash
# TEST-001: run the required lib suite twice under default parallelism.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

export MEDOUSA_TEST_HERMETIC=1
export CARGO_TERM_COLOR=never

run_pass() {
  local n="$1"
  echo "test-hermetic: pass ${n}"
  local extra=()
  if [[ -n "${RUST_TEST_THREADS:-}" ]]; then
    extra+=(-- --test-threads="${RUST_TEST_THREADS}")
  fi
  # Job timeout-minutes covers the suite; per-pass wall clock fails hung tests
  # before the second pass. macOS has no GNU timeout — CI is Ubuntu.
  if command -v timeout >/dev/null 2>&1; then
    timeout --signal=KILL 20m cargo test -p medousa --lib "${extra[@]}"
  else
    cargo test -p medousa --lib "${extra[@]}"
  fi
}

run_pass 1
run_pass 2
echo "test-hermetic: OK"
