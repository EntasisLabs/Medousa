#!/usr/bin/env bash
# Compare P01/P03 and governed-world samples against checked-in micro-CI ceilings.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"
export MEDOUSA_P01_CI=1
export CARGO_TERM_COLOR=never

budget="${repo_root}/scripts/ci/perf-budget.json"
if [[ ! -f "${budget}" ]]; then
  echo "check-perf-budgets: missing ${budget}" >&2
  exit 1
fi

p01_out="$(mktemp)"
p03_out="$(mktemp)"
world_authority_out="$(mktemp)"
world_observation_out="$(mktemp)"
trap 'rm -f "${p01_out}" "${p03_out}" "${world_authority_out}" "${world_observation_out}"' EXIT

echo "check-perf-budgets: P01"
cargo run -p medousa-engine --release --example p01_turn_stream >"${p01_out}"
echo "check-perf-budgets: P03"
cargo run -p medousa --release --example p03_feed_workspace >"${p03_out}"
echo "check-perf-budgets: governed world authority"
cargo bench -p medousa-world --bench hot_path >"${world_authority_out}"
echo "check-perf-budgets: cached semantic observation"
cargo bench -p medousa-browser-bridge --bench cached_observation >"${world_observation_out}"

python3 - "${budget}" "${p01_out}" "${p03_out}" "${world_authority_out}" "${world_observation_out}" <<'PY'
import json
import sys
from pathlib import Path

budget = json.loads(Path(sys.argv[1]).read_text())
p01_lines = Path(sys.argv[2]).read_text().strip().splitlines()
p03_lines = Path(sys.argv[3]).read_text().strip().splitlines()

def last_json(lines):
    for line in reversed(lines):
        line = line.strip()
        if line.startswith("{"):
            return json.loads(line)
    raise SystemExit("no JSON sample in probe output")

failed = False

p01 = last_json(p01_lines)
p01_ceil = budget["probes"]["p01"]
elapsed = float(p01["elapsed_ms"])
allocs = int(p01["allocations"])
print(f"P01 elapsed_ms={elapsed} allocations={allocs}")
if elapsed > float(p01_ceil["elapsedMs"]):
    print(f"P01 elapsed_ms {elapsed} exceeded ceiling {p01_ceil['elapsedMs']}", file=sys.stderr)
    failed = True
if allocs > int(p01_ceil["allocations"]):
    print(f"P01 allocations {allocs} exceeded ceiling {p01_ceil['allocations']}", file=sys.stderr)
    failed = True

p03 = last_json(p03_lines)
p03_ceil = budget["probes"]["p03"]
append_ms = float(p03["append_ms"])
print(f"P03 append_ms={append_ms} records={p03.get('records')}")
if append_ms > float(p03_ceil["appendMs"]):
    print(f"P03 append_ms {append_ms} exceeded ceiling {p03_ceil['appendMs']}", file=sys.stderr)
    failed = True

world_authority = last_json(Path(sys.argv[4]).read_text().strip().splitlines())
world_authority_ceil = budget["probes"]["worldAuthority"]
admission_p95_ns = int(world_authority["admission_completion_p95_ns"])
takeover_p95_ns = int(world_authority["human_takeover_p95_ns"])
print(
    "world authority "
    f"admission_completion_p95_ns={admission_p95_ns} "
    f"human_takeover_p95_ns={takeover_p95_ns}"
)
if admission_p95_ns >= int(world_authority_ceil["admissionCompletionP95Ns"]):
    print(
        "world authority admission p95 "
        f"{admission_p95_ns} did not stay below {world_authority_ceil['admissionCompletionP95Ns']}",
        file=sys.stderr,
    )
    failed = True
if takeover_p95_ns > int(world_authority_ceil["humanTakeoverP95Ns"]):
    print(
        "world authority human takeover p95 "
        f"{takeover_p95_ns} exceeded {world_authority_ceil['humanTakeoverP95Ns']}",
        file=sys.stderr,
    )
    failed = True

world_observation = last_json(Path(sys.argv[5]).read_text().strip().splitlines())
world_observation_ceil = budget["probes"]["worldCachedObservation"]
observation_p95_ns = int(world_observation["p95_ns"])
observation_nodes = int(world_observation["nodes"])
print(
    "world cached observation "
    f"p95_ns={observation_p95_ns} nodes={observation_nodes}"
)
if observation_nodes != int(world_observation_ceil["nodes"]):
    print(
        f"world cached observation sampled {observation_nodes} nodes, expected {world_observation_ceil['nodes']}",
        file=sys.stderr,
    )
    failed = True
if observation_p95_ns >= int(world_observation_ceil["p95Ns"]):
    print(
        "world cached observation p95 "
        f"{observation_p95_ns} did not stay below {world_observation_ceil['p95Ns']}",
        file=sys.stderr,
    )
    failed = True

sys.exit(1 if failed else 0)
PY
