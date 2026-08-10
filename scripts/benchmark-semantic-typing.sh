#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

measure() {
    local label="$1"
    shift
    printf '\n[%s]\n' "$label"
    /usr/bin/time -p "$@"
}

printf '%s\n' "Semantic typing benchmark (warm/incremental repository target)"
printf '%s\n' "Target directory: ${CARGO_TARGET_DIR:-${MEDOUSA_CARGO_TARGET_DIR:-../.cache/cargo-target}}"

measure "incremental library check" cargo check -p medousa --lib
measure "compatibility schemas and wire behavior" \
    cargo test -p medousa --lib typed_tools::compat::tests --quiet
measure "recurring construction and binding" \
    cargo test -p medousa --lib recurring_ --quiet
measure "runtime composition dispatch" \
    cargo test -p medousa --lib runtime_composition_ext::tests --quiet
measure "job specification construction" \
    cargo test -p medousa --lib runtime_job_spec::tests --quiet
measure "assembled first-party contract" \
    cargo test -p medousa --lib tool_contract_baseline::assembled_first_party_contracts_match_baseline --quiet
