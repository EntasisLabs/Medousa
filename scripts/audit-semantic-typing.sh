#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

count_matches() {
    local pattern="$1"
    shift
    rg -o --no-messages "$pattern" "$@" | wc -l | tr -d ' '
}

printf '%s\n' "Semantic typing audit (source baseline)"
printf '%-42s %s\n' "trim calls" "$(count_matches 'trim\(\)' src)"
printf '%-42s %s\n' "lenient compatibility helpers" "$(count_matches 'deserialize_lenient_' src)"
printf '%-42s %s\n' "direct PortFailure string mappings" "$(count_matches 'PortFailure\([^)]*to_string' src)"
printf '%-42s %s\n' "RuntimeComposition backend mentions" "$(count_matches 'RuntimeComposition::(InMemory|Surreal)' src)"
printf '%-42s %s\n' "NewJob literals" "$(count_matches 'NewJob[[:space:]]*\{' src)"
printf '%-42s %s\n' "RecurringDefinition literals" "$(count_matches 'RecurringDefinition[[:space:]]*\{' src)"
printf '%-42s %s\n' "ChannelDeliveryTarget literals" "$(count_matches 'ChannelDeliveryTarget[[:space:]]*\{' src)"
printf '%-42s %s\n' "too_many_arguments allowances" "$(count_matches 'allow\(clippy::too_many_arguments\)' src)"

recurring_paths=(
    src/recurring_delivery.rs
    src/recurring_feed.rs
)

printf '%s\n' ""
printf '%s\n' "Migrated recurring JSON boundary"
printf '%s\n' "Allowed legacy adapter reparsing (serde_json::from_value):"
if ! rg -n --no-heading 'serde_json::from_value' "${recurring_paths[@]}"; then
    printf '%s\n' "(none)"
fi

check_failed=0
if rg -n --no-heading 'serde_json::to_value' "${recurring_paths[@]}"; then
    printf '%s\n' "FAIL: typed recurring paths must not serialize values for internal reparsing" >&2
    check_failed=1
else
    printf '%s\n' "PASS: no typed recurring serialization round trip detected"
fi

if [[ "${1:-}" == "--check" && "$check_failed" -ne 0 ]]; then
    exit 1
fi
