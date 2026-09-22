# Benchmark Coder with Terminal-Bench

This is an operator workflow for measuring normal Medousa Coder mode with
[Terminal-Bench 4.0](https://www.tbench.ai/run). The adapter lives in
[`evals/terminal_bench`](../../evals/terminal_bench/agent.py) and uses Harbor's
[installed-agent interface](https://docs.harborframework.com/core-concepts/agents/custom-agents).

Each trial starts the ordinary `medousa_daemon --backend in-memory` inside the
task environment, with fresh data and config directories. It creates a session,
attaches the task checkout through `POST /v1/forge/items/start`, binds the session,
selects Coder, and submits the original instruction to
`POST /v1/interactive/turn`. The normal daemon owns the registry, Forge leases,
memory, prompts, and tool loop. The adapter follows the v3 event stream until a
terminal result. Production runtime code is unchanged.

Forge's existing `attached_checkout` mode keeps edits at the task's original
paths, where Harbor's verifier runs. A task directory without Git is initialized
and committed before attachment. Existing repositories retain their input files
and dirty state; detached HEADs get a local `codex/medousa-benchmark-*` branch.
Only use disposable benchmark environments with this runner.

## Install and run

Use Python 3.12 or newer on the Harbor host. Build `medousa_daemon` from the
revision you want to measure on Linux, using the repository's normal build:

```bash
cargo build --release --bin medousa_daemon
```

The binary must match the task container's CPU architecture and shared libraries.
The repository's Cargo configuration puts release artifacts in
`../.cache/cargo-target/release/` unless `CARGO_TARGET_DIR` overrides it. A macOS
daemon build cannot run in a Linux task container. Setup checks that the uploaded
binary starts before submitting a prompt.

From the repository root on the Harbor host:

```bash
python3 -m venv /tmp/medousa-tbench-venv
/tmp/medousa-tbench-venv/bin/pip install -r evals/terminal_bench/requirements.txt

# Set these to the Linux build and model being measured.
export MEDOUSA_BENCH_DAEMON=/absolute/path/to/linux/medousa_daemon
export MEDOUSA_BENCH_MODEL=provider/model
# Supply the chosen provider's normal API-key environment variable separately.

PYTHONPATH="$PWD" /tmp/medousa-tbench-venv/bin/harbor run \
  -d terminal-bench/terminal-bench@4.0.0 \
  -a evals.terminal_bench.agent:MedousaCoder \
  -m "$MEDOUSA_BENCH_MODEL" \
  --ak "daemon_path=$MEDOUSA_BENCH_DAEMON" \
  --ae 'OPENAI_API_KEY=${OPENAI_API_KEY}' \
  -e docker -n 1 -k 1
```

The environment-variable example is for OpenAI; replace it for your provider.
Harbor resolves the environment reference. Keep credentials out of constructor
arguments and prompts. The adapter installs Python and Git as needed and uploads
the prebuilt daemon and runner into the task environment.

Start with `-i TASK_NAME` for one chosen task. The default workspace is the task
container's working directory. If that is `/`, a subdirectory of a repository,
or otherwise unsuitable, pass `--ak workspace=/absolute/task/repository` for that
task. The adapter rejects the filesystem root and ancestor-repository attachment.
It does not relocate task files or rewrite paths in the instruction.

The complete 4.0 dataset includes GPU tasks. For a comparable full run, your
Harbor environment must supply each task's declared GPU and resource requirements;
a CPU-only subset is a smoke test. Preserve dataset resource limits and timeouts.
Set repetitions with `-k` and report them with the score; the official run example
uses five attempts. Do not report the single-attempt example above as equivalent
to that configuration.

## Settings and results

`-m` uses `MEDOUSA_PROVIDER_ID/MODEL_ID`; it selects the daemon's normal saved
provider/model settings. Optional agent arguments are:

| Argument | Meaning |
|---|---|
| `defaults_path` | Host path to an explicitly chosen `tui_defaults.json`; copied into fresh trial state before daemon startup |
| `reasoning_effort` | Normal Medousa reasoning setting |
| `base_url` | Provider endpoint reachable from inside the task environment |
| `workspace` | Absolute task repository path inside the environment |

The selected model and explicit reasoning/base-URL arguments override those fields
in `defaults_path`. Explicit stage routes in that file remain active; record all
models used when comparing results. Omit it to use fresh normal defaults. The
adapter leaves tool-round budgets, tool exposure, prompts, and retry behavior to
the daemon. Memory starts empty for each trial and works normally during it.
It neither imports personal memory nor seeds another trial's solution.

Harbor collects `agent/medousa/` under each trial's logs:

- `manifest.json`: daemon binary hash, model, backend, workspace, and baseline commit.
- `request.json`, `undertaking.json`, `turn.json`: the actual request and bindings.
- `events.jsonl`, `result.json`, `final.txt`: native events, completion outcome, and response.
- `daemon.log`, `turn_ledger/`: daemon diagnostics and native inference receipts.
- `workspace-status.txt`, `workspace.patch`: final Git status and tracked-file changes.
- `adapter-error.json`: infrastructure or protocol failure, when present.

The actual task filesystem, including untracked files and effects outside the
repository, remains in the environment for Harbor to score. The Git patch is a
diagnostic aid, not a replacement for the verifier. Use Harbor artifact collection
if you also need to retain particular output files after container teardown.

The runner reconnects to the same turn using the event cursor; it never resubmits
the prompt on a dropped stream. Human approval, budget approval, and credential
requests end the unattended attempt with `needs_input`. It does not approve them.
Daemon failures and truncated streams are not treated as completed answers. Harbor
owns the trial timeout and scoring; the adapter stops its daemon before verification.
Terminal agent outcomes are saved as metadata, independently of verifier reward.

Token/cost totals stay unknown in Harbor because the native turn ledger covers
tool-loop requests rather than every inference operation. Analyze the recorded
ledger with [`scripts/analyze-coder-usage.py`](../../scripts/analyze-coder-usage.py)
and retain its coverage notes.

## Validate the shim

```bash
python3 -m unittest discover -s evals/terminal_bench -p 'test_*.py' -v

# Optional: native daemon, real Coder tool execution, scripted local model.
# This does not call a paid model or produce a benchmark score.
MEDOUSA_BENCH_TEST_DAEMON=/absolute/path/to/medousa_daemon \
  python3 -m unittest evals.terminal_bench.test_native_daemon -v
```

The native smoke check runs on the host's native daemon build. It verifies that
Coder's real shell tool writes to the attached workspace, that the final response
arrives over SSE, and that the daemon is stopped afterward.
