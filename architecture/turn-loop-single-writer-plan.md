# Turn loop — single writer & explicit loop entry

**Status:** ✅ Complete (2026-06-07)

Replaces interim-heuristic loop management with a deterministic rule: **no tool call → EndTurn; any tool call → loop semantics**. Interim user-facing prose is declared via `cognition_turn_begin_work`, not inferred from text.

Related: [turn-state-machine-plan.md](turn-state-machine-plan.md), [turn-control-tools-plan.md](turn-control-tools-plan.md)

---

## Problem

Interactive turns showed assistant text swapping ~3× per turn. Root cause: **multiple producers writing the same AnswerBuffer** (stream deltas, `final_pending`, terminal `final`, continuation synthesis) with last-writer-wins `final_text` replace in Home.

Legacy interim heuristics (`looks_like_interim_status`) forced extra model rounds when the model spoke before calling tools — token burn and “competing personalities.”

---

## Design principles

| Principle | Meaning |
|-----------|---------|
| **Tool call = loop** | Zero invocations this turn → EndTurn on prose. Any tool → loop / receipt / fuse rules apply. |
| **Explicit control tools = FSM** | `begin_work`, `finish`, `request_more_rounds` — no prose NLP for continue/stop. |
| **Single writer commit** | Terminal body only from `cognition_turn_finish` or EndTurn delivery. Progress ≠ final. |
| **Status ≠ body** | `turn_progress` events set status (and optional empty-bubble preview), never `final_text` replace. |

---

## Control-plane lifecycle

```
begin_work(message)  →  turn_progress on bus (status / empty bubble)
     ↓
  [tools…]
     ↓
finish(message)      →  terminal commit (existing)
```

| Tool | Host bus | Worker lane |
|------|----------|-------------|
| `cognition_turn_begin_work` | ✅ | ✅ |
| `cognition_turn_finish` | ✅ | ✅ |
| `cognition_turn_request_more_rounds` | ✅ | ✅ |
| `cognition_turn_prepare_final` | ❌ removed | ✅ workshop only (deprecated) |

---

## FSM policy (shipped)

### No tool debt (zero invocations)

| Condition | Action |
|-----------|--------|
| `prepare_final` pending + non-empty draft (workshop) | EndTurn |
| At max rounds | EndTurn (fuse) |
| Clarifying question | EndTurn |
| **Otherwise** | **EndTurn** (`no_tools_prose`) |

### Tool debt (invocations exist)

| Condition | Action |
|-----------|--------|
| Workshop + `prepare_final` + non-empty draft | EndTurn |
| At max rounds | EndTurn (fuse) |
| Missing AVEC/calibrate receipts | ContinueLoop (MissingReceipts) |
| `prepare_final` + non-empty draft | EndTurn |
| Default | EndTurn |

---

## Bus events

| Event | `final_text` | UI |
|-------|--------------|-----|
| `content_delta` | — | Append preview |
| `turn_progress` | **no** | `statusLine`; fill content only if bubble empty |
| `final` | yes | Terminal merge (`resolveTurnContent.ts` / TUI reducer) |
| `scratch_reset` | — | Clear preview (Home + TUI) |

---

## Orchestrator

**Continuation synthesis** disabled on principal interactive surfaces (`interactive`, `tui`, `home-*`). Scheduled/headless lanes may still run it.

---

## Rollout

| Phase | Scope | Status |
|-------|--------|--------|
| **A** | Doc + `cognition_turn_begin_work` + `turn_progress` bus + FSM simplify | ✅ |
| **B** | Home/TUI reducers, drop `final_pending` body, disable interactive continuation | ✅ |
| **C** | Deprecate `prepare_final` on host; Home scratch_reset + terminal merge | ✅ |
| **D** | Host allowlist: `begin_work` in, `prepare_final` out; docs closed | ✅ |

---

## Success criteria (met)

- [x] Prose-only turn ends in one delivery (no interim continue)
- [x] Progress lines do not swap bubble body on terminal
- [x] Host cannot call `prepare_final` (workshop retains it)
- [x] Interactive continuation synthesis off
- [x] Home + TUI handle `scratch_reset` and terminal merge

---

## Phase 6 — Stream-first UX & partner voice (2026-06-08)

**Goal:** Keep streamed prose authoritative; cleaner worker synthesis; warmer collaborator voice.

| Change | Detail |
|--------|--------|
| Terminal merge | Prefer streamed body; replace only when stream was status-only or final extends it |
| Worker synthesis (Home) | One bubble — synthesis updates handoff message |
| Synthesis prompts | Same-thread integration; not cold rewrite |
| STTP | Sharp loyal partner tone — professional warmth, not flirtatious |

Async paths unchanged: `turn_progress`, `scratch_reset`, worker card delivery, background turns.

---

## Phase 7 — Actor loop, light touch (2026-06-09)

**Goal:** Principal chat feels like one Medousa — no stream/final dupe, less orchestration fighting the model. Keep host/worker delegation; trim merge heuristics and second-voice rewrites.

### Problem (post–Phase 6)

No-tool and simple turns still had **two body writers**: `content_delta` during LLM stream, then terminal `final` with overlapping or rewritten text. Home/TUI ran `resolveTurnContent` heuristics to pick a winner → visible swap at end of turn.

### Design principles

| Principle | Meaning |
|-----------|---------|
| **Stream is canonical** | On principal surfaces, if tokens streamed into the bubble, terminal `final` commits metadata only (tools, terminal flag, persist) — not a second body |
| **Progress ≠ answer** | `turn_progress` / `begin_work` → `statusLine` (or empty-bubble preview only); never compete with streamed answer |
| **Tools in structure** | Rich surfaces render tools from `tool_names` / `parts`; no markdown tool footer in canonical body (already P0) |
| **Host = actor** | Tool loop until `EndTurn` or `worker_spawned`; Medousa declares done via control tools — runtime does not NLP-merge prose |
| **Worker = delegate** | Spawn + card + synthesis when work ran; synthesis pass-through when worker already `finish`‑ed (Phase 7C, follow-up) |

### Phase 7A — Stream-authoritative terminal ✅

| Layer | Change |
|-------|--------|
| **Daemon sink** | Accumulate streamed markdown; persist streamed body; emit terminal `final` without `final_text` when stream delivered the answer |
| **Home** | Terminal merge: keep streamed bubble when non-empty; ignore redundant `final_text` |
| **TUI** | Same policy in `resolve_agent_turn_content` |

**Acceptance:** Plain chat turn — one stable bubble from first token through terminal; reload matches what user saw.

### Phase 7B — Orchestrator trim (follow-up)

- Confirm `maybe_append_tools_to_canonical_body` stays off for Home/TUI/interactive (already `RICH_SURFACE`).
- Gate receipt / AVEC continue loops to workshop lane only on host principal turns.

### Phase 7C — Worker synthesis pass-through (follow-up)

- Skip host synthesis LLM when worker output already terminal via `cognition_turn_finish`.
- Keep one-bubble handoff update in Home.

### Key files (Phase 7)

| Area | Path |
|------|------|
| Stream commit | `src/agent_runtime/daemon_interactive_turn.rs` |
| SSE final | `src/interactive_turn_runtime.rs` |
| Home reducer | `apps/medousa-home/src/lib/utils/resolveTurnContent.ts`, `chat.svelte.ts` |
| TUI reducer | `src/bin/medousa_tui/event_reducer.rs` |

---

## Key files

| Area | Path |
|------|------|
| Control tools | `src/turn_control_tools.rs` |
| Host allowlist | `src/agent_runtime/turn_worker/policy.rs` |
| FSM | `src/agent_runtime/turn_completion_fsm.rs` |
| Tool loop | `src/medousa_tool_loop.rs` |
| Home merge | `apps/medousa-home/src/lib/utils/resolveTurnContent.ts` |
| Home reducer | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Orchestrator | `src/agent_runtime/turn_orchestrator.rs` |
