# Worker continuity plan — same collaborator, workshop hat

> **Status:** Phase A in progress  
> **Date:** 2026-05-30  
> **Related:** [turn-worker-bus-plan.md](turn-worker-bus-plan.md), [context-lanes-and-scratchpad-plan.md](context-lanes-and-scratchpad-plan.md), [turn-worker-phase1.md](turn-worker-phase1.md)

## Problem

The host turn receives full STTP policy, identity context, memory recall, ambient blocks, and conversation `prior_messages`. The worker receives a ticket (`WORKER_TASK` + thin digests) and a “background specialist” prompt — it feels like a cold machine, not a continuation of the same Medousa.

Architecture doc Tier C already noted the gap: worker starts cold except task text.

## Principle

**Worker = Medousa in workshop mode.** Same persona and partnership energy; different lane (execution, not orchestration). Host = console; worker = workshop; synthesis = re-entry to the operator.

## Decisions (operator, 2026-05-30)

| Topic | Decision |
|-------|----------|
| Worker STTP | **Full STTP structure, curated for worker** — not the orchestrator prompt |
| Identity delegation graph | **In-process for now** — structured logs at spawn; Surreal relationships in Phase B |
| Conversation handoff | **Last 3–4 user/assistant turns** when payload is small; **1–2** when over budget. Future: Locus STTP handoff nodes |
| Concurrency | **Sequential workers only** for now; design leaves room for parallel workers with distinct identities later |

## Phases

### Phase A — Continuity bundle at spawn (highest ROI) ✅ shipped

Freeze `HostContinuityBundle` at host turn start and merge into `WorkerHandoffCapsule` at spawn:

| Field | Source |
|-------|--------|
| `identity_summary` | `PreparedTurnPrompt.identity_probe` |
| `recall_snippets` | `PreparedTurnPrompt.recall_probe` (top snippets) |
| `ambient_appendix` | `PreparedTurnPrompt.ambient_appendix` |
| `compiler_summary` | `PreparedTurnPrompt.compiler_output` |
| `vibe_signature` / `model_avec` | handoff (already present; duplicated for continuity block) |
| `recent_excerpts` | last 3–4 or 1–2 `ConversationTurn` (user/agent only) |
| `parent_turn_correlation_id` | `TurnContinuationScope` |

Deliverables:

- `src/agent_runtime/worker_continuity.rs` — bundle build, excerpt selection, logging
- `WORKER_STTP_POLICY` — curated STTP in `system_prompt.rs`
- Worker prompts rewritten — continuation voice, not “user not in thread”
- `◈ worker_continuity` / `◈ worker_delegation` notices + stderr logs

### Phase B — Identity store delegation edges

At spawn, record in-process `DelegationLink { parent_turn, work_id, intent, receipt }`. Later: commit `relationship_kind=delegation` to identity graph with `derived_from_relationship_id` and `transition_receipt_id`.

Worker/synthesis call `get_identity_context` including governing delegation edges.

### Phase C — Worker `prepare_turn_prompt` lite

Mirror host `identity_context_probe` + `cheap_memory_recall_probe` at worker start with lane profile `worker_research` / `worker_memory`. Add `cognition_identity_context` (read-only) to research allowlist.

### Phase D — Host spawn contract

Structured `task` with `why`, `resolved`, `do_not`, `success`. Host prompt requires continuation prose in spawn input.

### Phase E — Synthesis re-entry

Synthesis opens with delegated-turn framing; inject continuity bundle + worker scratch.

## Future

- Locus STTP compiler **handoff nodes** replacing raw excerpt window
- **Parallel workers** with distinct identity scopes per `work_id`
- Identity interruption policy when operator messages mid-worker (async mode B/C)

## Logging (Phase A, in-process)

```
◈ worker_continuity excerpts=4 recall_hits=2 identity=ready ambient=yes
◈ worker_delegation work_id=… parent_turn=… intent=research sequential=true
```

Stderr mirrors the same fields for daemon forensics without TUI noise.
