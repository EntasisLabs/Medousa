# Medousa Home — Main Workspace Plan (M6+)

> **Status:** Plan — post M5 polish; strategic gap closure  
> **Date:** 2026-05-30  
> **Related:** [medousa-home-plan.md](medousa-home-plan.md), [component-tui.md](component-tui.md), [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md), [turn-worker-bus-plan.md](turn-worker-bus-plan.md)

## Thesis

**Medousa Home must become the primary workspace** — not a pretty chat shell beside a terminal product.

Today that claim is **not true**:

| Surface | Role today | Honest score |
|---------|------------|--------------|
| **TUI** | Operator control room — observability, settings, routing, jobs, thinking, scripts | Runtime UX: **A** |
| **Home** | Workshop for chat, vault, kanban — calm but **blind** during turns | Presentation: **B+**, Runtime: **D** |
| **Daemon** | Full agent runtime, scheduler, identity, delivery, continuations | Capability: **A** — mostly **unexposed** in Home |

M5 fixed *trust and polish*. M6+ fixes *authority*: Home must show **what the runtime is doing** and expose **operator controls** without reintroducing TUI clutter.

**Principle (unchanged):** `medousa_daemon` owns truth. Home adds surfaces; it does not fork runtime logic.

---

## Three-layer gap model

### Layer 1 — Presentation gap (Home vs TUI)

What TUI shows that Home ignores or simplifies:

| TUI surface | What the operator sees | Home today |
|-------------|------------------------|------------|
| **Observability panel** | Tool invocations, job enqueue/process, receipts filter | Activity feed (summaries only) |
| **Thinking panel / peek** | Reasoning stream, scratch, gatekeeper gaps | Not shown (`reasoning_delta` dropped) |
| **Job history** | Recent jobs with type + status | Work cards only (projection) |
| **Settings overlay** | Provider, model, depth, tool rounds, stage routing, env | Connection URL + theme only |
| **Slash command plane** | `/model`, `/depth`, `/stage-routes`, `/daemon ask`, scripts | None |
| **Grapheme console** | Script/workflow output | None |
| **Verification detail** | Answer state, citation coverage | None |

**Key insight:** The daemon **already emits** rich `InteractiveTurnStreamEvent` fields (`phase`, `message`, `tool_names`, `reasoning_delta`). Home chat discards everything except `content_delta`. Fixing this is **M6a — zero new APIs**.

### Layer 2 — Control gap (neither UI fully exposes daemon)

Daemon routes that exist but Home does not call:

| API | Capability |
|-----|------------|
| `POST /v1/runtime/config/command` | Model, depth, tool policy at runtime |
| `POST /v1/runtime/stage-route/command` | Per-role provider/model routing |
| `GET /v1/stats` | Queue depth, scheduler tick |
| `GET /v1/delivery/status` | Outbox health |
| `GET /v1/continuations/status` | Turn continuation lineage |
| `POST /v1/jobs/ask`, `/report` | Fire-and-forget agent jobs |
| `POST /v1/recurring/prompt` | Schedule recurring work |
| `GET /v1/jobs/{id}/result` | Job output drill-down |
| `POST /v1/jobs/{id}/replay-and-resume` | DLQ recovery |
| `POST /v1/identity/*` | Identity context, propose/commit |
| `POST /v1/runtime/artifact/command` | Artifact inspection |
| `GET /v1/heartbeat/status` | Proactive heartbeat |

TUI reaches many of these via slash commands and daemon HTTP helpers. Home reaches **~15%** of the daemon surface.

### Layer 3 — Capability gap (Medousa > any single UI)

Runtime features that exist in the engine but no surface exposes well:

| Engine capability | Gap |
|-------------------|-----|
| **Host/worker bus** | Delegation, synthesis, worker tool policy — cards show status, not bus timeline |
| **Turn ledger / scratch** | Operator cannot see open gaps mid-turn |
| **Verifier / answer_state** | Evidence trail not in Home |
| **Recurring + delivery** | No schedule view, no delivery ack visibility |
| **OpenShell / skill sandbox** | Skills panel runs; no live sandbox output |
| **MCP gateway** | Policy evaluate API unused in Home |
| **Identity graph** | No people/preferences UI |
| **Stage routing matrix** | TUI-only settings |

Closing Layer 3 requires **new daemon projections** (SSE channels, aggregated runtime status) — not just more buttons in Svelte.

---

## North star — Home as control room

```text
┌────┬────────────────────────────────────────────┬──────────────┐
│Nav │  Primary surface                           │ Right stack  │
│    │  Chat · Library · Work · Skills · Home     │ Context      │
│    │                                            │ Activity     │
│    │  [Turn awareness strip when streaming]     │ Runtime ◀──  │
├────┴────────────────────────────────────────────┴──────────────┤
│ Status: Connected · N in motion · delivery ok · scheduler tick │
└────────────────────────────────────────────────────────────────┘
```

**Runtime drawer** (new, M6b): collapsible right stack or dedicated nav — not a sixth homepage, a **drawer** like TUI overlays:

- **Now** — active turn phases, tools, worker status
- **Jobs** — queue + recent (from stats + workspace)
- **Delivery** — outbox, channel acks
- **Controls** — model, depth, routing (daemon config commands)
- **Doctor** — health snapshot (links to `medousa doctor` output shape)

Chat stays default. Runtime is **one click away**, not a separate app.

---

## Milestones

### M6a — Turn awareness (ship first — **shipped slice 1**)

*Use existing SSE; no daemon changes.*

| # | Work | Exit |
|---|------|------|
| M6a.1 | Chat: show `phase` + `message` during stream | Operator sees "tool_loop", "synthesis", etc. |
| M6a.2 | Chat: tool name chips from `tool_names` | "Using: cognition_vault_search, …" |
| M6a.3 | Optional reasoning peek (`reasoning_delta` accordion) | TUI thinking peek parity (lite) |
| M6a.4 | Activity: map `event_type` / phases to human labels | Feed matches turn strip |

**Touch:** `chat.svelte.ts`, `ChatPanel.svelte`, `types/chat.ts`

### M6b — Runtime drawer (daemon APIs) — **shipped**

| # | Work | API | Status |
|---|------|-----|--------|
| M6b.1 | `GET /v1/stats` + health enrichment | Tauri command | ✓ |
| M6b.2 | Runtime panel — Now / Jobs / Delivery / Controls / Routing tabs | `RuntimePanel.svelte` | ✓ |
| M6b.3 | `GET /v1/delivery/status`, `GET /v1/continuations/status` | Status bar whisper + Delivery tab | ✓ |
| M6b.4 | Settings → Controls: model + depth via `runtime/config/command` | Wired to `interactive_turn_send` | ✓ |
| M6b.5 | Stage routing read-only view | `stage-route/command` Routes | ✓ (edit deferred) |

### M6c — Job & worker drill-down — **shipped**

| # | Work | Status |
|---|------|--------|
| M6c.1 | Card inspector: `GET /v1/jobs/{id}/result` output panel | ✓ |
| M6c.2 | Worker bus timeline on card detail (feed + tool_names) | ✓ |
| M6c.3 | Batch blocked actions (retry group, dismiss group) | ✓ |
| M6c.4 | `/ask` / `/daemon ask` fire-and-forget from chat composer | ✓ |

### M6d — Scheduler & recurring — **shipped**

| # | Work | Status |
|---|------|--------|
| M6d.1 | `GET /v1/recurring` + Runtime Schedule tab | ✓ |
| M6d.2 | Schedule manuscript from Skills panel | ✓ |
| M6d.3 | Next-run whisper on Home hero | ✓ |

### M6e — Identity & evidence — **shipped**

| # | Work | Status |
|---|------|--------|
| M6e.1 | Identity drawer — recall context for session | ✓ |
| M6e.2 | Verifier / answer_state on assistant messages | ✓ |
| M6e.3 | Artifact links on card detail | ✓ |

### M6f — TUI repositioning

| Decision | Recommendation |
|----------|----------------|
| Keep TUI? | Yes — power-user / SSH / minimal env |
| Default onboarding | `medousa setup` → **Home** on desktop; TUI optional |
| Parity rule | Every TUI slash command maps to Home action or Runtime drawer within 2 releases |

---

## API additions needed (daemon-first)

Before M6c–M6e UI, prefer these daemon projections:

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/runtime/status` | Aggregated: active turns, queue, delivery, last tick (one poll for drawer) |
| `GET /v1/runtime/settings` | Read model, depth, routing (mirror of config command state) |
| `GET /v1/recurring` | List recurring definitions — **shipped** |
| `GET /v1/jobs/recent` | Thin job list for Runtime drawer (or extend workspace feed) |
| SSE `runtime://events` | Optional: push tool invocations globally (Activity enrichment) |

---

## Success metrics

1. During a tool-heavy turn, Home shows **which tools** ran — without opening TUI
2. Operator changes model/depth in Home → next turn uses new config (daemon confirmed)
3. Card inspector shows **job result text**, not just status label
4. README positions Home as **primary workspace**; TUI as advanced/terminal
5. New user never needs terminal to understand "what Medousa is doing"

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | Main workspace plan — gap analysis, M6a–M6f milestones |
| 2026-05-30 | M6b shipped — Runtime nav, daemon stats/delivery/continuations, model/depth controls |
| 2026-05-30 | M6c shipped — Job result panel, worker timeline, batch blocked actions, /ask composer |
| 2026-05-30 | M6d shipped — GET /v1/recurring, Skills schedule, Home next-run whisper |
| 2026-05-30 | M6e shipped — Identity drawer, answer_state badges, artifact previews on cards |
