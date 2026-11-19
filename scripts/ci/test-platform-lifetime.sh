#!/usr/bin/env bash
# Actual daemon constructors register process-global handlers/stores. Each exact
# ignored test MUST execute in its own process, not alongside the general lib suite.
# This compiles Rust: do not run on disk-constrained correction hosts without budget.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
export MEDOUSA_TEST_HERMETIC=1
export CARGO_TERM_COLOR=never
# Real constructor futures overflow the default libtest thread stack in debug builds.
# Fix this isolated harness at 32 MiB (including if a caller supplies another value);
# do not change production stacks or require an external environment workaround.
export RUST_MIN_STACK=33554432
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
for name in \
  startup_queues_worker_continuations_without_executing_them \
  retained_agent_survives_final_platform_drop_without_observation_failure \
  late_actual_constructor_failure_closes_observations_after_handler_registration \
  polled_actual_constructor_cancellation_closes_observations_after_handler_registration
do
  cargo test --locked -p medousa --no-default-features --features full-daemon --lib \
    "runtime::platform::lifetime_tests::${name}" -- --ignored --exact --test-threads=1 | tee "$log"
  # A stale exact selector otherwise exits successfully while running zero tests.
  grep -q '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$log"
done
