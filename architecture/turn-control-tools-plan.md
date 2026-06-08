# Turn control tools — budget extension & explicit finish

Internal plan for two model-facing control-plane tools that give Medousa better self-management of turn loops: escape overly tight round limits without silent failure, and end turns immediately when the completion gatekeeper misjudges output.

## Problem

1. **Over-limiting** — classifier/host-bus/activation paths sometimes cap tool rounds too aggressively for the task at hand.
2. **Forced continue** — gatekeeper/heuristics keep the loop alive after the model has a complete answer → token blow-up and latency.

## Existing baseline

| Mechanism | Behavior |
|-----------|----------|
| `cognition_turn_prepare_final` | Sets flag; **next text-only** message may finalize (still subject to gatekeeper) |
| `[MEDOUSA_TURN_CONTROL]` | Injects round budget awareness each model round |
| `ApprovalRequired` (MCP) | Side-effect tool approval; separate concern |
| Workspace **Blocked** column | Home pulse for operator decisions (ask jobs, failures) |

`prepare_final` does **not** solve forced-continue pain — it still requires another LLM round and gatekeeper pass.

---

## Tool B — `cognition_turn_finish` (Phase 1) ✅

## Tool A — `cognition_turn_request_more_rounds` (Phase 2) ✅

**Purpose:** Model asks for budget extension instead of burning stuck continues or failing at the fuse.

**Input:**
```json
{
  "requested_rounds": 3,
  "reason": "Need 2 more MCP reads + synthesis",
  "progress_summary": "Completed X; still need Y"
}
```

**Flow:**
1. Read `rounds_executed`, `max_tool_rounds`, channel, session, turn correlation id.
2. Create `TurnBudgetRequest` (pending).
3. **Pause** tool loop (checkpoint messages, scratchpad, invocations).
4. Notify operator on origin channel; fallback to workspace **Blocked** card (Home).
5. Tool output: `{ ok: false, status: "pending_approval", request_id, current: "4/8" }`.

**Approval:**
- **Approve:** bump `max_tool_rounds` for this turn only (capped), resume from checkpoint.
- **Deny:** resume with current budget; inject control message to finish or ask one question.
- **Timeout (v2):** optional auto-deny.

**Policy caps:** e.g. +8 per request, max 2 approvals/turn, absolute ceiling 32. Extensions are per-turn, not global settings.

### New modules (Phase 2)

- `TurnBudgetRequestStore` — Surreal + in-mem fallback
- `TurnLoopPauseHandle` — in-process checkpoint + resume in daemon
- Channel notification router (TUI event, Home blocked card, Telegram/Discord/Slack)
- HTTP: `GET/POST /v1/turns/budget-requests/{id}/approve|deny`

---

## Tool C — `cognition_turn_begin_work` (Phase 5) ✅

**Purpose:** Explicit tool-loop entry signal with a principal-facing progress line. Replaces interim-heuristic continues.

**Input:** `{ "message": "…", "intent": "optional log" }`

**Flow:** After tool batch → `turn_progress` bus event (status only). Loop continues via tool debt.

See [turn-loop-single-writer-plan.md](turn-loop-single-writer-plan.md).

---

## Tool B — `cognition_turn_finish` (Phase 1) ✅ in progress

**Purpose:** Model ends the turn **immediately** with final user-facing text in the tool call — bypasses gatekeeper “continue” misjudgments. No extra model round.

**Input:**
```json
{
  "message": "Complete user-facing answer…",
  "reason": "optional log note"
}
```

**Flow:**
1. Validate non-empty `message`.
2. After tool batch, if `finish_turn` present → return `ToolLoopExecutionResponse` with that text.
3. `termination_reason: cognition_turn_finish`
4. Append assistant turn via existing terminal path (orchestrator / daemon sink).
5. **Bypass gatekeeper** on this path (Phase 1). Receipt checklist hard-blocks deferred to Phase 2 config if needed.

**Coexistence with `prepare_final`:**
- `prepare_final` — streaming “wrapping up”, next prose round
- `finish_turn` — hard stop with payload in tool; use when gatekeeper keeps looping

---

## Integration map

```
execute_local_turn → MedousaToolLoopPipeline
  ├─ cognition_turn_finish → immediate Ok (Phase 1)
  ├─ cognition_turn_request_more_rounds → pause (Phase 2)
  └─ gatekeeper / heuristics (unchanged fallback)
```

**Key files:**

| Area | Path |
|------|------|
| Tools | `src/turn_control_tools.rs` |
| Loop exit | `src/medousa_tool_loop.rs` |
| Registration | `src/tui/runtime_services.rs` |
| Prompts | `turn_ledger.rs`, `system_prompt.rs`, worker prompts |
| Ledger | `turn_ledger.rs` (`Finalized` detail) |
| Phase 2 store | `src/turn_budget_request_store.rs` (new) |
| Phase 2 workspace | `src/workspace/card.rs` |
| Phase 2 API | `daemon_handlers.rs`, `daemon_api.rs` |

---

## Rollout

| Phase | Scope |
|-------|--------|
| **1** | `cognition_turn_finish` + loop hard-stop + prompts + tests |
| **2** | `cognition_turn_request_more_rounds` + inline pause/wait/resume + workspace blocked cards + daemon approve/deny + Home UX |
| **3** | Channel push (Telegram/Discord/Slack/WhatsApp) with `medousa://work/{request_id}` deep link; Home surfaces tagged `home-desktop` / `home-ios` / `home-android` |

### Home channel surfaces

| Surface | When |
|---------|------|
| `home-desktop` | Tauri macOS/Windows/Linux or browser workshop |
| `home-ios` | Tauri iOS shell |
| `home-android` | Tauri Android shell |
| `home` | Legacy alias → normalized to `home-desktop` |

Interactive turns from Medousa Home send `TurnSurfaceContext.channel_surface`; the daemon resolves `delivery_target` from that (no longer hardcoded `tui`). Budget cards show the channel in subtitle; external ingest channels get a push message with deep link when the turn pauses.

---

## Tests

- Phase 1: tool invoke validation; `finish_turn_from_invocations`; loop terminates without extra LLM call (unit tests in `turn_control_tools.rs`, `medousa_tool_loop.rs`)
- Phase 2: Surreal in-mem store; approve/deny resume; workspace projection
