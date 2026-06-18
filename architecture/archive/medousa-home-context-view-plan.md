# Medousa Home — Context View Plan (M12)

> **Status:** **M12 complete + human polish** · **M13a Map shipped** (2026-06-07)  
> **Map plan:** [medousa-home-context-map-plan.md](medousa-home-context-map-plan.md)
> **Date:** 2026-06-07  
> **Epic:** **M12 — Context view (witness memory)**  
> **Builds on:** [M11 charter plan](medousa-home-m11-settings-charter-plan.md), workshop coherence pass  
> **Related:** [cognitive-identity-memory-plan.md](cognitive-identity-memory-plan.md), [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md)

## North star

**Context** is where the operator **witnesses what Medousa remembers** — identity and episodic memory — without editing charter (Settings) or engine telemetry (Runtime).

**Mantra:** Resonantia is the atlas. Medousa Home is the window into the shelf.

**Steve test (epic exit):** Operator opens Context and sees *what she carries into the room* in human language first. Plumbing (STTP layers, entity IDs) is collapsed unless they ask. Read-only — no coach, no “she thinks you…”

---

## Product boundary

| Surface | Question | Owns |
|---------|----------|------|
| **Settings** | *How should she behave toward me?* | Charter — memory windows, voice, reach |
| **Context** | *What does she remember?* | Identity recall + Locus threads + posture (read-only) |
| **Runtime** | *What's the engine doing?* | Jobs, delivery, routing telemetry |
| **Resonantia** | *How does my mind feel as navigable terrain?* | Full constellation / descent experience (separate app) |

Chat **Identity recall** drawer remains a quick peek; **Open in Context →** promotes to the full surface.

---

## Resonantia inspiration (borrow vs. leave)

### Borrow (workshop-native)

| Resonantia | Medousa Context |
|------------|-----------------|
| Context is terrain | Group recall by kind; search; master-detail pull-in |
| Descent, not page churn | List → detail in one pane; map stays in list |
| Witness, not coach | Read-only; human summary first |
| STTP layers (⊕ ⦿ ◈ ⍉) | Collapsible detail sections; advanced collapsed |
| AVEC fingerprint | Posture tab (M12c) — whisper strip, not playground |

### Leave for Resonantia

- Full-screen constellation / 3D descent
- No search as primary
- “AI never speaks” positioning
- Crypto metrics as hero marketing
- Replacing Chat / Work / Library with one continuous mind-map

---

## Navigation

**Desktop:** Life-orbit rail item **Context** (icon: `Orbit`) — after Library, before Skills tier divider.

**Mobile:** You hub → **Context** row under “Stay in touch” or new “Memory” section.

**Not** in Settings. **Not** in Runtime.

Header line: *What she carries into the room — identity and episodic memory.*

---

## Three tabs (master-detail shell)

Same pattern as Messaging / Cron: `.workshop-list-pane` + `.workshop-detail-pane` + `.workshop-list-row-active`.

| Tab | ID | Human name | Source | Row |
|-----|-----|------------|--------|-----|
| **Recall** | `recall` | What she knows about you | `identity_get_context` (cognitive) | Claim, contact, relationship, persona summary |
| **Threads** | `threads` | Session memory | Locus STTP list API (M12b) | Session, tier, excerpt, time |
| **Posture** | `posture` | How you showed up | Session AVEC / envelope (M12c) | Session, stability · friction · … |

### Detail pane layers (Resonantia-shaped, Medousa-skinned)

```
Human summary (always open)
▸ Envelope   ids · status · timezone
▸ Provenance claim_id · graph depth (when applicable)
▸ Raw        collapsed JSON for power users
```

Default: human open, plumbing collapsed.

---

## Epic phases

| Phase | Name | Ship | Depends on |
|-------|------|------|------------|
| **M12a** | Shell + Recall | Nav, `ContextPanel`, Recall list/detail, refresh, mobile You row | Existing `identity_get_context` |
| **M12b** | Threads | Locus node list + STTP detail | Daemon read API `POST /v1/locus/nodes/list` (+ get by id) |
| **M12c** | Posture | AVEC strip per session | Memory context or session envelope fields |
| **M12d** | Cross-links | Search polish, “Open chat session”, vault associations | Workspace + vault APIs |

---

## M12a — Shell + Recall (shipping)

### Touch

| File | Role |
|------|------|
| `types/ui.ts` | `Surface` += `"context"` |
| `types/context.ts` | Tab IDs, recall entry types |
| `utils/contextRecall.ts` | Build searchable recall rows from identity context |
| `stores/identity.svelte.ts` | `refresh()` global cognitive load |
| `components/context/ContextPanel.svelte` | Header, tabs, layout |
| `components/context/ContextRecallList.svelte` | Master list |
| `components/context/ContextRecallDetail.svelte` | Detail inspector |
| `layout/NavSidebar.svelte` | Orbit nav item |
| `layout/WorkshopShell.svelte` | Route surface |
| `types/mobile.ts` + `YouHub.svelte` | Mobile destination |
| `chat/IdentityDrawer.svelte` | “Open in Context →” link |
| `chat/ChatPanel.svelte` | `onOpenContext` prop |

### Recall row kinds

- `claim` — flattened cognitive claims
- `contact` — people in graph
- `relationship` — social / trust edges
- `persona` — single row when present
- `user` — operator profile row when present

### Verification

```bash
cd Medousa/apps/medousa-home && npm run check && npm run build
```

Manual:

1. Rail → Context opens panel with charter header + three tabs (Threads/Posture disabled whispers)
2. Recall tab loads identity cognitive context; list → detail selection works
3. Search filters claims/contacts/relationships
4. Chat identity drawer → “Open in Context →” lands on Context → Recall
5. Mobile You → Context opens embedded panel

---

## M12b — Threads (shipped)

### Daemon

- `GET /v1/locus/nodes?session_id=&limit=&q=` — read-only list (newest-first via `ContextQueryService`)
- `GET /v1/locus/nodes/{sync_key}` — detail with raw STTP body

### Home

| File | Role |
|------|------|
| `src/locus_handlers.rs` | HTTP handlers |
| `daemon_api.rs` | `LocusNodeSummary`, list/detail types |
| `daemon/locus.rs` | Tauri invoke bridge |
| `types/locus.ts` | Frontend types |
| `stores/contextThreads.svelte.ts` | Threads store |
| `utils/contextThreads.ts` | Row builders |
| `ContextThreadsList/Detail.svelte` | Master-detail UI |
| `ContextPanel.svelte` | Threads tab enabled |

Detail pane uses Resonantia-shaped layers: Envelope · Provenance · Content · Metrics · Raw.

---

## M12b — Threads (original plan notes)

---

## M12c — Posture (shipped)

Per-session AVEC fingerprints derived from the latest Locus node with a `user_avec` envelope in each session.

| Piece | Role |
|-------|------|
| `utils/contextPosture.ts` | Group nodes by session; build searchable posture rows |
| `stores/contextPosture.svelte.ts` | Loads Locus list (limit 120) |
| `ContextPostureFingerprint.svelte` | Resonantia-inspired bar strip (stability · friction · logic · autonomy · ψ) |
| `ContextPostureList/Detail.svelte` | Master-detail; model posture when present |
| `ContextPanel.svelte` | Posture tab enabled; session names from chat store |

No new daemon endpoint — posture is computed client-side from existing `/v1/locus/nodes`.

---

## M12c — Posture (original plan notes)

- Per-session AVEC from memory context tool output or persisted session envelope
- Read-only fingerprint chip row (stability, friction, logic, autonomy, ψ)

---

## M12d — Cross-links (shipped)

Witness navigation between tabs — read-only, no edit surfaces.

| Piece | Role |
|-------|------|
| `utils/contextCrossLinks.ts` | Claim→thread scoring, chat session lookup, search query helper |
| `ContextCrossLinks.svelte` | Shared action link row |
| `ContextPanel.svelte` | Tab routing, session filter chip, Locus warm-cache on Recall |
| `ContextRecallDetail.svelte` | Related threads list + open/search actions |
| `ContextThreadsDetail.svelte` | Open in Chat · View session posture |
| `ContextPostureDetail.svelte` | View threads · Open latest thread · Open in Chat |
| `WorkshopShell.svelte` / `YouHub.svelte` | `onOpenChat(sessionId)` → chat surface + session switch |

Cross-link matrix:

- **Recall claim** → related threads (keyword/claim-id match) · search threads · open best thread
- **Thread detail** → open chat (when session exists locally) · view session posture
- **Posture detail** → filter threads to session · open latest thread · open chat
- **Threads list** → session filter chip (from posture/recall navigation) with clear

### Verification

```bash
cd Medousa/apps/medousa-home && npm run check && npm run build
```

Manual:

1. Recall claim with Locus data shows related threads; “Search threads →” switches tab with query
2. Thread detail “Open in Chat →” lands on matching session
3. Posture detail “View session threads →” filters Threads tab; chip clears filter
4. Mobile You → Context cross-links switch to Chat tab with session

---

## M12 polish — Human-first (shipped)

Steve test pass: detail panes lead with memory, not machinery.

| Piece | Role |
|-------|------|
| `utils/contextHuman.ts` | Friendly time, session names, tier labels, STTP memory extraction, posture feel copy |
| `ContextWitnessHero.svelte` | Shared human header (title · meta · lead) |
| `ContextPlumbingSection.svelte` | Collapsed “If you need the machinery” block |
| Detail + list components | Renamed layers, pill nav links, shelf copy throughout |

Default detail hierarchy: **title → when/where whisper → memory lead → nav pills → plumbing collapsed**.
