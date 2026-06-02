# Turn completion gatekeeper

## Role

Symmetric to the **input intent classifier**: a small completion pass decides whether a text-only tool-loop round should **end the turn** or **continue** (more tools / another model round).

## Layers

1. **Receipt checklist** (code) — e.g. AVEC+calibrate prompts require `cognition_memory_calibrate` before end.
2. **Heuristics** (`turn_text_heuristics.rs`) — interim vs substantive draft text.
3. **`cognition_turn_prepare_final`** — model requests curtain; gatekeeper may veto if receipts fail.
4. **Gatekeeper model** (budgeted) — JSON `end_turn` | `continue` on triggers (stutter, ritual missing, prepare_final, fuse).
5. **TUI scratch reset** — `AgentScratchReset` clears in-flight bubble before next round (in-place replace).

## Wiring

- `ToolLoopCompletionGate` passed from `execute_local_turn` into `MedousaToolLoopPipeline::execute_with_stream_prior_messages_max_rounds`.
- Notices: `◈ completion gatekeeper decision=... source=... reason=...`
- Termination: `gatekeeper_receipt_checklist` | `gatekeeper_gatekeeper_model` | heuristic paths unchanged.

## Budget

Interactive lane: `max_gatekeeper_calls: 2` (shares `max_llm_calls_total` pool).
