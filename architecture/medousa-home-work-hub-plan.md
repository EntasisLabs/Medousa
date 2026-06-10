# Medousa Home — Work Hub Plan (M8)

> **Status:** In progress — W1 UI shell shipped first  
> **Date:** 2026-06-07  
> **Related:** [medousa-home-polish-plan.md](medousa-home-polish-plan.md), [medousa-home-main-workspace-plan.md](medousa-home-main-workspace-plan.md), [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md)

## North star

**Chaos → stone.** Work is not a kanban board. Work is the **homespace navigation hub** for everything Medousa and you manifest together.

The graph is a **mental model**, not a visualization gimmick:

- Each **ask** becomes a **manifestation card** on the main stage.
- **Status** moves as a chip (queued → running → finishing → needs you) — not as column geography.
- **Timeline, tools, turns, vault links** live *inside* the card pop-out — the data we already collect.
- **Settled / failed / stopped / stuck** collapse into **bottom trays** — cold memory, not screaming columns.
- Tapping a link (vault note, chat session, artifact) **navigates the homespace** — the graph is the state machine of navigation.

**Chat** = duet (language, now).  
**Work** = structure (manifestations, provenance, settlement).  
Shared ritual: **one ask composer**, chat-grade polish.

---

## Why kanban failed

| Kanban lie | Work Hub truth |
|------------|----------------|
| Equal columns for unequal weight | Main stage = hot; trays = cold |
| Blocked column as visual hero | Failure peripheral, grouped, honest |
| Dismiss without persistence | Archive/tombstone must be real (W2) |
| Swimlanes / refresh in header | Quiet ops; relationship title |
| Split inspector pane | Pop-out with transition — one place |
| Decorative empty columns | Empty = absence, not four "EMPTY" tombs |

Backend columns (`backlog`, `in_flight`, `wrapping_up`, `blocked`, `done`) **remain daemon truth**. UI **projects** them into hub semantics — it does not fork the runtime.

---

## Layout (desktop)

```text
┌──────────────────────────────────────────────────────────────┐
│ Work                                              [quiet ops] │
├──────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐           │
│  │ Card title  │  │ Card title  │  │ Card title  │  ← grid │
│  │ ● Finishing │  │ ● Running   │  │ ● Queued    │           │
│  │ chat · vault│  │ 12 tools    │  │ session     │           │
│  └─────────────┘  └─────────────┘  └─────────────┘           │
│                    (horizontal scroll grid)                     │
├──────────────────────────────────────────────────────────────┤
│  [Ask composer — chat-grade, one voice]                       │
├──────────────────────────────────────────────────────────────┤
│ Settled (12) │ Failed (3) │ Stopped (1) │ Stuck (74) ▾       │
└──────────────────────────────────────────────────────────────┘
```

### Locked UX decisions (2026-06-07)

1. **Main stage:** horizontal grid of manifestation cards (not vertical kanban columns).
2. **Detail:** expand as **pop-out overlay** with transition animation (not split inspector rail).
3. **Backend contract:** dismiss/settle/archive must persist before UI claims "cleared" (W2).
4. **Projection layer:** `WorkManifestation` view model composes card + detail + associations + timeline (W1–W3).

---

## Card model

### Living card (main grid)

Included when column ∈ `{backlog, in_flight, wrapping_up}` **or** blocked with `needs approval` (budget interrupt).

Sorted: `in_flight` → `wrapping_up` → `backlog` → `updated_at` desc.

### Status chip (UI)

| Daemon state | Chip label | Tone |
|--------------|------------|------|
| backlog | Queued | muted |
| in_flight | Running | primary pulse |
| wrapping_up | Finishing | amber |
| blocked + needs approval | Needs you | warning |
| blocked + other | Stuck | danger |

Column is **never** shown as geography on the card face — only the chip.

### Provenance row (card subtitle)

Tappable chips from `WorkCardDetail.associations` + session:

- `Chat` → open session in Chat
- vault path → Library note
- `{n} tools` → opens pop-out on tools section
- manuscript / specialist when present

### Pop-out contents (reuse CardInspector data)

- Status + actions (retry, cancel, approve budget, archive ask)
- **Timeline** — workspace feed filtered to card (`filterCardTimeline`)
- **Tools** — tool names + lineage when available
- **Associations** — vault paths, artifacts, locus nodes
- **Result excerpt** / job output
- Links outward = homespace navigation

---

## Bottom trays

| Tray | Source | Default |
|------|--------|---------|
| **Settled** | `column === done` | Collapsed, count whisper |
| **Failed** | `blocked` + `failed` / `dead_letter` | Collapsed |
| **Stopped** | `canceled` status | Collapsed |
| **Stuck** | remaining `blocked` | Collapsed; group when >5 |

Trays are **memory drawers**, not WIP columns. Opening a tray shows compact rows; selecting opens the same pop-out.

---

## Navigation graph (mental model)

Edges are **typed links**, not force-directed nodes:

```text
Ask card ──chat──► Session
    │
    ├──vault──► Library note
    ├──artifact──► Preview / artifact command
    ├──tools──► Pop-out tool timeline
    └──timeline event──► Same card, scrolled
```

Future: STTP node, identity slice, specialist halo as **provenance on edges** — not peers on a hairball graph.

---

## Backend contract (W2 — required for trust)

| Operator action | Required daemon behavior | API today |
|-----------------|-------------------------|-----------|
| **Settle / clear done ask** | Archive job + remove from living projection | `POST /v1/jobs/{id}/archive` ✓ |
| **Dismiss stuck** | Cancel or tombstone; card leaves living + tray rules | `POST /v1/workspace/cards/{id}/cancel` (partial) |
| **Hide from hub** | Persistent `archived` / `dismissed_at` on card projection | **Gap — add W2** |
| **Retry** | Replay job / re-queue ask | `POST .../retry` ✓ |

Until W2: UI copy says **Hide** / **Cancel**, not **Dismiss**, when persistence is cancel-only.

---

## WorkManifestation projection (W3)

```typescript
interface WorkManifestation {
  id: string;
  title: string;
  statusChip: { label: string; tone: string };
  updatedAt: string;
  provenance: ProvenanceChip[];
  timeline: WorkspaceEvent[];
  detail?: WorkCardDetail;
  layer: "living" | "settled" | "failed" | "stopped" | "stuck";
}
```

Built from existing `WorkCard`, `WorkCardDetail`, `WorkspaceEvent[]`, `cardDetailsCache` — no new runtime fork.

---

## Milestones

| Milestone | Scope | Exit |
|-----------|-------|------|
| **W1** | Hub UI: grid, trays, pop-out, composer polish; kanban retired from WorkPanel | Work opens to grid + trays; card pop-out animates |
| **W2** | Archive/dismiss persistence; tray actions honest | Archive ask jobs from settled tray; hide stuck uses cancel |
| **W3** | `WorkManifestation` projection; timeline spine in pop-out; provenance navigates | Pop-out leads with timeline + links; mobile hub grid |
| **W4** | Mobile Work hub parity; kill kanban components | Single work surface mobile + desktop |
| **W5** | Activity rail + status bar link into hub trays | "74 need attention" → Stuck tray |

---

## Files (W1 touch map)

| Area | Files |
|------|-------|
| Plan | `architecture/medousa-home-work-hub-plan.md` |
| Partition / sort | `lib/utils/workHub.ts` |
| Hub shell | `lib/components/work/WorkHub.svelte` |
| Grid card | `lib/components/work/WorkManifestCard.svelte` |
| Main grid | `lib/components/work/WorkHubStage.svelte` |
| Bottom trays | `lib/components/work/WorkHubTrays.svelte` |
| Pop-out | `lib/components/work/WorkManifestPopover.svelte` |
| Panel wire | `lib/components/work/WorkPanel.svelte` |
| Styles | `app.postcss` (`.work-hub-*`, `.work-manifest-popover`) |
| Retired from Work | `KanbanBoard.svelte` (keep until W4 delete) |

---

## Principles (carry forward)

1. **Customer never sees plumbing** — chip says "Finishing", not `wrapping_up`.
2. **Motion is the hero** — trays collapsed by default.
3. **One ask voice** — same composer spirit as Chat.
4. **Provable settlement** — no decorative dismiss.
5. **Homespace hub** — Work links outward; it does not dead-end.

---

## Changelog

| Date | Note |
|------|------|
| 2026-06-07 | W1 shipped — grid, trays, pop-out, composer |
| 2026-06-07 | W3 shipped — ManifestTimeline, workManifestation projection, mobile hub grid |
