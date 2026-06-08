# Turn completion gatekeeper

> **Migration:** Output-side loop control lives in [turn-state-machine-plan.md](turn-state-machine-plan.md). Phase 1–2 FSM owns text-only round decisions; this gatekeeper model layer is **off the tool-loop hot path** (Interactive `max_gatekeeper_calls: 0`). Kept for Scheduled lane fallback and tests until Phase 3 removal.

## Role

Symmetric to the **input intent classifier**: a small completion pass decides whether a text-only tool-loop round should **end the turn** or **continue** (more tools / another model round).

## Layers

1. **Receipt checklist** (code) — e.g. AVEC+calibrate prompts require `cognition_memory_calibrate` before end.
2. **Heuristics** (`turn_text_heuristics.rs`) — interim vs substantive draft text.
3. **`cognition_turn_prepare_final`** — model requests curtain; gatekeeper may veto if receipts fail.
4. **Gatekeeper model** (budgeted) — JSON `end_turn` | `continue` on triggers (stutter, ritual missing, prepare_final, fuse).
5. **TUI scratch reset** — `AgentScratchReset` clears in-flight bubble before next round (in-place replace).
6. **Phase 0 turn ledger** — on `continue`, inject `[MEDOUSA_TURN_CONTROL]` into tool-loop `messages` and append JSONL events (see [turn-ledger-phase0.md](turn-ledger-phase0.md)).

## Wiring

- `ToolLoopCompletionGate` passed from `execute_local_turn` into `MedousaToolLoopPipeline::execute_with_stream_prior_messages_max_rounds`.
- Notices: `◈ completion gatekeeper decision=... source=... reason=...`
- Termination: FSM reasons (`tool_debt_complete`, `receipt_checklist` continues) via [turn-state-machine-plan.md](turn-state-machine-plan.md). Legacy gatekeeper paths below apply only when `resolve_turn_completion` is invoked elsewhere.

## Budget

Interactive lane: `max_gatekeeper_calls: 0` (FSM owns completion). Scheduled lane: `1` (legacy fallback in `resolve_turn_completion`).
