# Turn ledger — Phase 0 (implemented)

Phase 0 of [turn-worker-bus-plan.md](turn-worker-bus-plan.md): loop discipline on the **existing monolithic tool loop**, daemon-wide (all comms adapters).

## What shipped

### 1. Structured turn ledger (persisted)

- Module: `src/agent_runtime/turn_ledger.rs`
- Per-session append-only JSONL: `~/.local/share/medousa/turn_ledger/{session_id}.jsonl`
- Event kinds: `tool_round`, `text_only_continue`, `gatekeeper_continue`, `receipt_missing`, `finalized`, `stuck`
- Wired when `ToolLoopCompletionGate.session_id` is set (interactive turns via `execute_local_turn`)

### 2. Model-visible control messages (in-loop)

On continue (gatekeeper or interim heuristic), the tool loop appends a **system** line:

```
[MEDOUSA_TURN_CONTROL]
…guidance…
```

Phase 1–2 adds **`[MEDOUSA_SCRATCH]`** (goal, phase, step, tool digests, open_gaps) in the **tool lane only** — see [context-lanes-and-scratchpad-plan.md](context-lanes-and-scratchpad-plan.md). Ledger records may include a `scratch` JSON snapshot on `tool_round` / gatekeeper events.

The model sees why the turn did not end (missing calibrate, stutter, status-only text, etc.). Interim assistant text is still **not** appended to `messages` (see [tool-loop-interim-text-fix.md](tool-loop-interim-text-fix.md)).

### 3. Stuck detector

- After **max_tool_rounds** consecutive text-only continues **without new tool invocations** (same budget as configured tool rounds), the loop stops with `termination_reason=stuck_text_only_continue` and a user-facing message citing that limit (not a hardcoded “3”).
- Constant: `MAX_TEXT_ONLY_STUCK_CONTINUES` in `turn_ledger.rs`
- Observability: `◈ turn loop stuck: …` notice when a stream sink is present

### 4. Receipt checklist extension

- AVEC ritual + `pull` / `preset` in user prompt now requires `cognition_memory_moods` (or context tool) before finalize, in addition to `cognition_memory_calibrate`.
- Logic: `missing_ritual_tools_for_avec` in `turn_completion.rs`

## Wiring

| Component | Change |
|-----------|--------|
| `medousa_tool_loop.rs` | `TurnLoopDiscipline`, inject control messages, ledger + stuck |
| `turn_completion.rs` | `ToolLoopCompletionGate.session_id`, extended checklist |
| `turn_orchestrator.rs` | `LocalTurnExecutionParams.session_id`, gate session id |
| `daemon_interactive_turn.rs` | `AssembleLocalTurnParams.session_id` |
| `medousa_tui/agent_runtime.rs` | Pass `session_id` into local turn params |

## Not in Phase 0

- Host/worker delegation (Phase 1)
- Bus events on `AgentStreamSink` as structured SSE types (notices only today)
- Ledger replay into session `ConversationTurn` history (sidecar file only)

## Verify locally

```bash
cargo test turn_ledger turn_completion medousa_tool_loop
cargo check
```

After a ritual-heavy turn, inspect `~/.local/share/medousa/turn_ledger/<session>.jsonl` and obs for `◈ completion gatekeeper` / `◈ turn loop stuck`.
