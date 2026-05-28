# Medousa

Medousa is a cognitive runtime surface built on top of Stasis orchestration primitives.

It is designed for practical, evidence-aware assistant operation across three surfaces:

- `medousa_tui`: interactive operator workspace
- `medousa_cli`: one-shot command runner and daemon client
- `medousa_daemon`: long-running API and scheduler process

The product model is straightforward:

1. keep interaction ergonomic for day-to-day use
2. keep execution durable and inspectable
3. keep evidence and confidence visible when needed

## What Medousa Does

Medousa combines:

- conversational prompting with tool-loop orchestration
- artifact capture and chunk references for large payloads
- extraction, verification, and context-pack composition flows
- role-based stage routing controls
- progressive disclosure of answer state and verification signals

In practice, this means you can run normal assistant turns, inspect what happened, and tune behavior without leaving the product surface.

## Capability Snapshot

Core interaction:

- interactive chat with streaming responses
- slash commands for runtime, artifacts, verification, export, and control
- command palette and keyboard-first overlays

Evidence and trust:

- payload receipts and artifact persistence
- chunk references and extraction support
- verification scoring and verification lineage records
- answer-state labeling (`verified` or `provisional`) in chat output

Routing and behavior control:

- role-based routing matrix for stage roles
- per-role provider/model/policy/fallback controls
- response depth controls (`concise`, `standard`, `deep`)

Operations:

- daemon endpoints for ask/prompt/recurring workflows
- scheduler loop for recurring materialization and processing
- backend parity for `in-memory` and `surreal-mem`

## Quick Start

## 1) Run the TUI

```bash
cargo run -p medousa --bin medousa_tui
```

Common in-TUI commands:

- `/settings` for runtime + routing controls
- `/history` for session history
- `/artifact-*` and `/verify-*` for evidence/verification workflows
- `/depth concise|standard|deep` for response depth behavior

## 2) Run a one-shot CLI prompt

```bash
cargo run -p medousa --bin medousa_cli -- llm "Summarize this runtime state in 5 bullets"
```

## 3) Run daemon mode

```bash
cargo run -p medousa --bin medousa_daemon -- --backend in-memory
```

Then call daemon endpoints from CLI:

```bash
cargo run -p medousa --bin medousa_cli -- daemon-health
```

## Runtime Configuration

Provider/model can be set by flags or environment.

Common examples:

```bash
export STASIS_LLM_PROVIDER=openai
export STASIS_LLM_MODEL=gpt-4o-mini
```

Ollama example:

```bash
export STASIS_LLM_PROVIDER=ollama
export STASIS_LLM_MODEL=llama3.2
export MEDOUSA_OLLAMA_BASE_URL=http://localhost:11434/v1/
```

## Typical Usage Flows

## Flow A: Interactive investigation loop (TUI)

1. Start in chat and ask a question.
2. Use observability output to inspect tool/runtime behavior.
3. Use artifact and verification commands to inspect trust signals.
4. Adjust routing/settings/depth as needed.

## Flow B: Script execution + validation

1. Open editor (`/edit` or `/open`).
2. Run script (`/run` or `/run-current`).
3. Review runtime diagnostics and job outcomes.
4. Persist/export relevant output.

## Flow C: Service operation (daemon)

1. Start daemon for continuous scheduling.
2. Enqueue ask/prompt/recurring work via API or CLI client commands.
3. Track health/stats and outcomes through API + runtime logs.

## What to Expect

Behavioral expectations:

- durable execution semantics come from Stasis job lifecycle
- tool and runtime diagnostics are first-class and visible
- answer confidence can vary by evidence availability and policy settings

Operational expectations:

- `in-memory` backend is fast for local work and iteration
- `surreal-mem` is useful for more durable runtime workflows
- settings changes in TUI rebuild runtime composition where applicable

## Persistence and Data Locations

TUI-managed local data is stored under:

- `~/.local/share/medousa/history/`
- `~/.local/share/medousa/tui_defaults.json`
- `~/.local/share/medousa/last_session`
- `~/.local/share/medousa/secrets/api_key` (file fallback when keyring is unavailable)

API keys use OS keyring when available.

## Architecture References

For technical internals:

- [architecture/README.md](architecture/README.md)
- [architecture/enterprise-architecture-and-flow-guide.md](architecture/enterprise-architecture-and-flow-guide.md)
- [architecture/system-overview.md](architecture/system-overview.md)
- [architecture/component-tui.md](architecture/component-tui.md)
- [architecture/component-cli.md](architecture/component-cli.md)
- [architecture/component-daemon.md](architecture/component-daemon.md)
- [architecture/interaction-and-state-model.md](architecture/interaction-and-state-model.md)