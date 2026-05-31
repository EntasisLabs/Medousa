# Centralized Agent Runtime — Roadmap

> Created: 2026-05-31  
> Status: Planned (successor to outbox delivery)  
> Related: [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md), [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md), [component-tui.md](component-tui.md)

## Thesis

The **TUI local agent runtime** is the gold standard: tool-loop orchestration, activation heuristics, memory/identity probes, context packs, continuation synthesis, lane budgets, and stage routing.

Telegram, Discord, CLI, and daemon ingest should **not** run separate brains (`workflow.stasis.agent_session` jobs, or the simplified `PromptExecutionPipeline`-only interactive turn). They should all execute the **same agent turn loop**, hosted by the daemon, with channel adapters as thin ingress/egress.

Outbox → channel delivery (Phase 5 ingester track) solved **how replies reach users**. This track solves **what generates the reply**.

## Current State (three brains, one product)

| Surface | Execution path today | Tooling / loops |
|---------|----------------------|-----------------|
| **TUI (preferred)** | `POST /v1/interactive/turn` → direct LLM stream | Minimal — no tool loop |
| **TUI (fallback)** | Local `build_tui_runtime` → `ToolLoopPipeline` | **Gold standard** |
| **Ingest (Telegram/Discord/CLI)** | `agent_session` Stasis job on scheduler | Different shape; mock search tool only |

Verified May 2026: outbox delivery works after diagnostics extraction fix, but ingest still does not match TUI quality or architecture.

## Target Architecture

```
Any adapter (TUI / Telegram / Discord / CLI)
        ↓
POST /v1/agent/turn  (or unified /v1/interactive/turn)
        ↓
Daemon AgentRuntime (extracted from TUI)
  · activation + intent classifier
  · context pack + verifier + memory/identity probes
  · ToolLoopPipeline + stage routing
  · streaming SSE events
        ↓
On terminal: outbox JobSucceeded → /v1/deliver/outbox → channel push
        ↓
Adapter: ack + typing only (final text from delivery)
```

**Principles**

1. **One turn engine** — daemon-owned, session-scoped, channel-agnostic.
2. **TUI becomes a client** — renders stream events; does not own orchestration logic long-term.
3. **Adapters stay thin** — forward in, render out; no job polling, no duplicate LLM paths.
4. **Delivery stays authoritative** — outbox push completes the user-visible turn (already built).

## Extraction Scope (what moves out of TUI)

| Today (TUI-local) | Target (shared) |
|-------------------|-----------------|
| `src/bin/medousa_tui/agent_runtime.rs` (~2k lines) | `src/agent_runtime/turn_orchestrator.rs` |
| `src/bin/medousa_tui/turn_services.rs` | `src/agent_runtime/turn_services.rs` |
| `src/tui/runtime_services.rs` + `TuiRuntime` assembly | `src/agent_runtime/runtime.rs` (`MedousaAgentRuntime`) |
| `src/tools.rs` cognition tool registry wiring | `src/agent_runtime/tools.rs` (shared registry builder) |
| TUI-specific `TuiEvent` emissions | `AgentStreamEvent` (reuse/extend `InteractiveTurnStreamEvent`) |

**Stays in TUI:** panel state, scrolling, overlays, keyboard routing, obs log formatting.

**Stays in daemon (already):** ingest routing, session mapping, delivery registry, outbox webhook, scheduler.

## Phases

### Phase 0 — Delivery foundation ✅

See [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md).

- Outbox publish → internal webhook → channel dispatch
- Thin adapters + deliver poll fallback
- Temporary `agent_session` diagnostics extraction (bridge only)

**Exit criteria:** Telegram roundtrip delivers real text; outbox `pending → published`.

---

### Phase 1 — Extract the gold standard ✅

Move TUI agent orchestration into `src/agent_runtime/` without changing behavior.

- [x] Create `src/agent_runtime/` module tree
- [x] Define `MedousaAgentRuntime` (tool loop, memory, identity, registry — today’s `TuiRuntime` minus TUI coupling)
- [x] Define channel-agnostic `AgentTurnRequest` / `AgentStreamEvent` (session, prompt, routing, depth, identity hints)
- [x] Move turn services: activation, prior messages, pipeline selection, intent context (from `turn_services.rs`)
- [x] Port turn orchestration: context pack, continuation, budgets, intent classifier (from `agent_runtime.rs`)
- [x] TUI calls extracted module in-process (no daemon change yet) — behavior must match current local fallback pixel-for-pixel on test prompts
- [x] Unit tests on activation and prior-message slicing (no TUI harness required)
- [x] Unit tests on continuation gate

**Exit criteria:** TUI local fallback uses `medousa::agent_runtime::*` only; no orchestration logic left in `medousa_tui/agent_runtime.rs` except UI glue. ✅ (Phase 1 slice 3)

---

### Phase 2 — Daemon hosts the runtime ✅ **Done**

Run the extracted runtime inside `medousa_daemon`.

- [x] Upgrade `/v1/interactive/turn` to invoke `MedousaAgentRuntime` (replaces bare `PromptExecutionPipeline` path)
- [x] Wire session_id, stage routing, response depth from interactive turn request
- [x] Stream `content_delta`, `reasoning_delta`, `status`, `final`, `error` events (same SSE contract TUI/adapters consume)
- [x] Daemon builds shared agent runtime at startup (`build_daemon_agent_runtime`)
- [x] Register delivery target + pending delivery record on interactive turn accept (`InteractiveTurnDeliveryContext`)
- [x] TUI primary path → daemon agent turn; `--local-runtime-only` / `MEDOUSA_TUI_LOCAL_RUNTIME=1` for offline/dev fallback
- [x] Doctor + `daemon-health`: agent runtime version, last turn latency, tool registry count

**Exit criteria:** TUI chat via daemon matches local fallback quality on tool-using prompts; dashboard shows one code path in logs. ✅

---

### Phase 3 — Channel convergence ✅ **Done (ingest path)**

Point all ingress at daemon agent turns.

- [x] Ingest `EnqueueAsk` → start agent turn (not `agent_session` Stasis job)
- [x] Remove ingest job poll task (`run_ingest_job_stream_task` deleted)
- [x] CLI `daemon-ask` uses agent turn + delivery poll (via `/v1/ingest`)
- [x] Discord/Telegram: unchanged adapter shell; daemon does the thinking
- [x] Session history: append assistant turn on delivery completion (`IngestAgentStreamSink`)

**Exit criteria:** Telegram, Discord, CLI, and TUI all hit the same daemon route; no `workflow.stasis.agent_session` ingest jobs in dashboard. ✅

---

### Phase 4 — Decommission legacy paths ✅ **Done**

- [x] Remove simplified `run_interactive_turn_stream_task` (PromptExecutionPipeline-only) — replaced in Phase 2
- [x] Remove ingest `AgentSessionJobPayload` builder in `start_ingest_ask_stream`
- [x] Migrate `POST /v1/jobs/ask` and `/v1/jobs/report` off `for_agent_session` → `spawn_daemon_api_agent_turn`
- [x] Remove bridge diagnostics extraction for `turns[].response_text`
- [x] Update [component-daemon.md](component-daemon.md) and [component-tui.md](component-tui.md)
- [x] Archive notes in [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md) Phase 6 → superseded

**Exit criteria:** Single agent turn implementation in repo; grep for `for_agent_session` in ingest/daemon paths returns zero. ✅

---

### Phase 5 — Parity & hardening

- [ ] Per-session stage routing + depth from ingester (`/model`, `/depth`) applied to agent turn
- [ ] Verifier + answer_state surfaced in delivery metadata (optional channel formatting)
- [ ] Heartbeat/proactive messages through agent runtime policy (not ad-hoc strings)
- [ ] Load/soak tests: concurrent Telegram chats + TUI + scheduler tick
- [ ] Performance alignment with [tui-performance-target-plan.md](tui-performance-target-plan.md)

**Exit criteria:** Wizard + doctor report agent runtime health; no feature regression vs today’s TUI local fallback.

## Non-Goals (this track)

- Rewriting Stasis job orchestration internals
- Running tool loop inside Telegram adapter process
- Removing Stasis scheduler (still needed for recurring/report background jobs)
- Removing outbox delivery (remains the completion contract)

## Migration Notes

| Legacy | Replacement |
|--------|-------------|
| `workflow.stasis.agent_session` ingest jobs | Daemon `MedousaAgentRuntime` turn |
| `/v1/interactive/turn` bare LLM stream | `/v1/agent/turn` full runtime |
| TUI `build_tui_runtime` in hot path | Daemon-hosted; local only offline |
| Ingest SSE job polling | Agent turn SSE + outbox delivery |

## Code Anchors (today)

| Area | Path |
|------|------|
| TUI turn orchestration (extract source) | `src/bin/medousa_tui/agent_runtime.rs` |
| Turn activation / prior messages | `src/bin/medousa_tui/turn_services.rs` |
| Runtime assembly | `src/tui/runtime_services.rs`, `src/tools.rs` |
| Daemon simplified turn (replace) | `run_interactive_turn_stream_task` in `medousa_daemon.rs` |
| Ingest agent_session (replace) | ~~`start_ingest_ask_stream`~~ → `run_agent_turn` + `IngestAgentStreamSink` |
| Delivery (keep) | `src/channel_delivery.rs`, outbox webhook |

## Suggested Phase 1 PR Slices

1. **Scaffold** — `src/agent_runtime/mod.rs`, `MedousaAgentRuntime` type alias/wrapper, no behavior change
2. **Turn services** — move `turn_services.rs` + tests
3. **Orchestrator core** — move activation, prior messages, context pack, continuation (no TuiEvent; callback/trait for stream sink)
4. **TUI rewiring** — TUI imports shared module; delete duplicated code from bin
5. **CI** — prompt fixture tests comparing output structure pre/post
