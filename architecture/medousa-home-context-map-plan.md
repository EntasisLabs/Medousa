# Medousa Home — Context Map (M13)

> **Status:** **M13a shipped** + **M13a polish** (collapsed map, human labels, map detail)  
> **Date:** 2026-06-07  
> **Epic:** **M13 — Context map (shelf overview)**  
> **Builds on:** [context view plan](medousa-home-context-view-plan.md)

## North star

**Map** is the overview lens on the shelf — see how sessions and moments connect without leaving Medousa Home.

**Mantra:** Resonantia is the atlas. Home Map is the view from across the room.

**Steve test:** Operator opens Map, recognizes their life in clusters in under 3 seconds, clicks one moment, lands in a human one-beat detail pane — no UUIDs, no hairball, no machinery up front.

## Product boundary

| Surface | Role |
|---------|------|
| **Recall / Threads / Posture** | List → detail witness (default) |
| **Map (M13a)** | Session nodes · expand for moments · click → map detail |
| **Resonantia** | Full constellation / 3D descent (separate app) |

Leave for later: vault note nodes (M13c), claim nodes (M13b), force-layout hairballs, 3D.

## M13a — Session map (shipped)

### Graph model

- **Session node** — one per `session_id`, labeled from chat display names (never raw UUID)
- **Thread nodes** — up to 16 per session when session expanded (or search-revealed)
- **Edges** — session → moment (solid membership), moment → moment (dashed time sequence)

### M13a polish (shipped)

- **Collapsed by default** — sessions only; tap to expand moments (no hairball)
- **Human labels** — `humanMomentTitle`, `sessionMapLabel`; opaque IDs banned from map + titles
- **Label hierarchy** — session labels primary; moment labels whisper until selected
- **Visible edges** — stronger stroke weights; solid membership, dashed sequence
- **Map detail** — `ContextMapMomentDetail`: title · when · one sentence · cross-links · Technical drawer
- **Search** — filters sessions; matching moments appear without manual expand

### M13a graph navigation (shipped)

- **Session chain** — chronological links between sessions always visible (even collapsed)
- **Sized nodes** — session radius ∝ √moment count; thread radius ∝ tier + signal
- **Hue per session** — stable color per session cluster
- **Pan / zoom** — drag canvas, scroll wheel, +/−/fit controls
- **Smart labels** — show on hover, selection, zoom, or hub size; stroke halo for overlap
- **Large layout** — graph spreads in virtual space instead of squashing to viewport

### M13a exploration UX (shipped)

- **Ghost satellites** — collapsed sessions show up to 8 faint moment nodes + membership edges (structure without expand)
- **Auto-expand recent 5** — latest sessions open fully on first Map visit
- **Hover neighborhood** — dim graph, brighten connected nodes/edges; hover card with link summary
- **Click = focus** — selects node, animates camera to neighborhood, opens moment detail when applicable
- **Double-click session = expand/collapse** — all moments for that session (separate from focus)
- **No click-to-unlock** — relationships visible on arrival and on hover

### M13a finish polish (shipped)

- **Clear focus** — click empty canvas, ✕ control, Esc, or pinned card link; camera eases back to full graph
- **Kind palette** — violet circles = sessions, teal rounded squares = moments (`contextMapVisual.ts`)
- **Legend** — Session · Moment · Memory · Note (latter two reserved for M13b/M13c)
- **Per-kind styling** — shape, hue, glow, and membership edge color differ by node type

### Interaction

- Tap session → expand/collapse its moments
- Tap moment → load map detail in right pane
- Search filters visible clusters/nodes
- Refresh loads Locus list + chat sessions

### Touch

| File | Role |
|------|------|
| `types/context.ts` | `map` tab |
| `utils/contextMap.ts` | Graph build, layout, filter |
| `utils/contextHuman.ts` | `humanMomentTitle`, `sessionMapLabel`, `looksLikeOpaqueId` |
| `ContextMapCanvas.svelte` | SVG renderer + label hierarchy |
| `ContextMapView.svelte` | Legend, empty/loading, expand state |
| `ContextMapMomentDetail.svelte` | One-beat map detail pane |
| `ContextPanel.svelte` | Map tab + split detail |

### Verification

```bash
cd Medousa/apps/medousa-home && npm run check && npm run build
```

Manual:

1. Context → Map shows **sessions only** by default (with moment counts)
2. Tap session → moments expand with readable edges
3. Click moment → human map detail (no sync_key in headline)
4. Search reveals matching moments without hairball; mobile tap → detail

## M13b — Memory links (planned)

- Recall claim nodes + claim↔thread edges from cross-link scoring

## M13c — Vault bridge (planned)

- Note nodes from vault + note↔session/thread links
