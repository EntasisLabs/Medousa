# Tool loop: interim assistant text — fix

## Symptom

During an interactive agent turn (TUI, Telegram ingest, daemon interactive), the model sometimes:

1. Streams a short status update (“Let me check that…”)
2. Then attempts tool calls on the **next** model round

The turn ended after step 1 with no tools run. If the model only called tools and replied at the end, behavior was fine.

## Root cause

`MedousaToolLoopPipeline` in `src/medousa_tool_loop.rs` treated **any** model response with assistant text and **zero** `tool_calls` as a final answer:

```text
tool_calls.is_empty() && maybe_text.is_some() → return Ok (terminate)
```

That is correct **after** tools have run (`invocations` non-empty) for a **substantive** synthesis. It is wrong **before** any tools when the model is still working (status preamble), and wrong **between** tools on short acks (“Stored.”).

## Fix history

| Version | Behavior | Problem |
|---------|----------|---------|
| v1 | Continue text-only only before first tool | Finalized on short acks after tools |
| v2 | Continue until `max_tool_rounds`; append interim to `messages` | Turn no longer killed early, but model “talks to itself” because interim text became a dialog turn |
| **v3** | Heuristic finalize + **never append interim to `messages`** | Matches human chat: status is ephemeral; only final answer closes the turn |

## Fix (v3 + explicit prepare_final tool)

### `cognition_turn_prepare_final`

Control-plane tool (`src/turn_control_tools.rs`) the model calls when tool work is done. Sets `pending_final_answer` in the tool loop; the **next** non-empty text-only response finalizes the turn (`termination_reason: prepare_final_then_text`). Alias: `cognition.turn.prepare_final`. Classified read-only in `execution_policy.rs`.

If the model calls other tools after `prepare_final`, the flag is cleared (still working). Heuristics and max-round fuse remain when the tool is not used.

## Fix (v3 — heuristics)

### Finalize when (text-only, no `tool_calls`)

| Condition | Finalize? |
|-----------|-----------|
| `has_selected_tool` (single-tool legacy path) | No — legacy fallback handles |
| `invocations.is_empty()` | No — status / preamble before tools |
| `looks_like_interim_status(text)` | No — short ack / procedural line (before or after tools) |
| Substantive text after ≥1 tool invocation | Yes |
| `rounds_executed >= max_tool_rounds` | Yes — safety fuse only |

### Transcript rule

Interim status may stream via `content_chunk` (TUI / SSE). It must **not** be pushed as `ChatMessage::assistant(...)` when continuing the loop — that pollutes the thread and causes self-dialogue on the next API call.

### Turn-loop awareness (AX)

Each model round starts with a compact `[MEDOUSA_TURN_CONTROL]` system line (`TurnLoopAwareness` in `turn_ledger.rs`):

- **Tool rounds remaining** in this turn (`max_tool_rounds - rounds_executed`).
- **User-visible responses sent** this turn (interim scratch streamed to TUI/SSE), with the **first 100 characters** of the last reply as a preview.
- On the **last** round, an explicit warning that tools-only on that round ends the turn without a final reply.

Gatekeeper / heuristic continue messages prepend the same budget block so the model knows where it is without re-injecting full interim text into the transcript.

### TUI-configurable limits (`TurnLoopSettings`)

All caps below are in **Settings → Runtime** (saved to `tui_defaults.json`). Each turn logs `◈ turn_loop_limits …` with the resolved values.

| Setting | Default | What it caps |
|---------|---------|----------------|
| Max Tool Rounds | 10 | Base budget before activation/host bus |
| Host Bus Max Tool Rounds | 8 | Orchestrator slim-host cap when bus is active |
| Host Turn Bus Mode | auto | `auto` / `force` / `off` (env `MEDOUSA_TURN_HOST_BUS` still overrides) |
| Activation Tool-Intent Max Rounds | 12 | Heuristic when prompt looks tool-heavy |
| Activation Short-Turn Max Rounds | 1 | Short direct-answer / long-session turns |
| Continuation Max Tool Rounds | 4 | Post-turn continuation synthesis loop |
| Max Text-Only Stuck Continues | 10 | Interim replies without new tools |
| Classifier Restricted Max Rounds | 1 | Low-confidence / conversational classifier paths |
| Retry Runtime Max Rounds | 10 | Retries after runtime `PortFailure` |

### Prompt policy (`[MEDOUSA_TOOL_POLICY]`)

Interactive tool-loop turns append `append_tool_loop_policy` to the user prompt (orchestrator + TUI + workers): `max_tool_rounds=N` and instructions to answer on the last round (or `cognition_turn_prepare_final` before it). Stuck / user-visible stop messages use the **configured** `max_tool_rounds`, not a hardcoded “3 tries”.

### Recoverable tool errors (host bus + tool loop)

When `ToolRegistry::invoke_tool` returns `PortFailure` (disallowed tool on host profile, MCP/Grapheme errors, validation, etc.), `MedousaToolLoopPipeline` no longer aborts the turn. It injects a tool receipt:

```json
{ "ok": false, "error": "...", "recoverable": true, "hint": "..." }
```

The model sees the failure on the next round and can adjust, retry once, or spawn a worker — matching `[MEDOUSA_HOST_BUS]` / worker Grapheme playbooks. Loop-level failures (max rounds, strict mode with no tool call, empty model response) still surface as `PortFailure` and follow retry policy where applicable.

### Heuristics

**`looks_like_interim_status`** — work-in-progress phrases **anywhere** in the text (“let me”, “i'll”, “lock it in”, “calibrating”, …), short acks, or ≤6 words.

**`looks_like_substantive_final_answer`** (required after ≥1 tool, unless `prepare_final` or max-round fuse) — not interim, ≥12 words, and either ≥20 words or outcome hints (stability, drift, calibrat, memory, …).

**Activation** — prompts mentioning `calibrat`, `avec`, `memory`, `pull`, `focus`, etc. classify as `tool_intent_detected` (full tool rounds, not long-session `enforce_no_tools`).

## Related: recurring agent turns

Scheduled recurring jobs default to `workflow.stasis.prompt` (one LLM call, no Medousa tool registry).

Use `execution_mode: "agent_turn"` on `POST /v1/recurring/prompt` (or register tools with `job_type` / payload) to run `workflow.medousa.recurring_agent_turn`, which calls `run_agent_turn` and the same tool loop per tick.

See [recurring-delivery-roadmap.md](recurring-delivery-roadmap.md) Phase 3.
