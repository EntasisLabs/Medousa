# Medousa Home — Polish Plan (M4)

> **Status:** Shipped (M4a–M4c, 2026-05-30)  
> **Date:** 2026-05-30  
> **Related:** [medousa-home-tauri-design.md](medousa-home-tauri-design.md), [medousa-home-plan.md](medousa-home-plan.md)

## North star

The README promises: **chaos → stone**. Calm, finished, provable.

Medousa Home must feel like a **study at night** — black glass, violet accent, one clear next action — not a daemon log viewer painted red.

**Design mantra:** The customer never sees the plumbing.

---

## Visual direction — Obsidian (replaces Sahara)

Sahara served scaffolding. Ship a **custom Skeleton theme** tuned for Medousa Home.

### Palette intent

| Role | Direction | Notes |
|------|-----------|-------|
| **Canvas** | Near-black `#0a0a0f` – `#121218` | Not warm brown/red — cool neutral dark |
| **Surfaces** | 3 steps only: base / raised / overlay | Stop red-on-red sameness |
| **Accent** | Violet `#8b5cf6` – `#a78bfa` (primary actions, selection, links) | User-requested contrast |
| **Secondary accent** | Muted lavender for chips, badges | Sparingly |
| **Success** | Cool green (connected, done) | Rare |
| **Warning** | Amber (wrapping up only) | Rare |
| **Danger** | Rose/coral (failed, cancel zone) | Rare — not the default background temperature |

### Typography

| Level | Use | Rule |
|-------|-----|------|
| **Title** | Surface headers, card titles | One weight step up, never ALL CAPS blocks |
| **Body** | Chat, editor, inspector | 14–15px, comfortable line height |
| **Whisper** | Timestamps, meta, footer | 11–12px, `surface-400` — never URLs at whisper size in the hero strip |

### Implementation

1. Add `apps/medousa-home/src/medousa-theme.ts` — Skeleton v2 custom theme (`CustomThemeConfig`)
2. Register in `tailwind.config.ts` as `custom: [medousaTheme]`; set `data-theme="medousa"` in `app.html`
3. Remove sahara preset (or keep as dev fallback only)
4. Settings: rename toggle to **Obsidian theme**; optional accent intensity later
5. Audit components: replace yellow `variant-filled-primary` abuse with violet primary; reserve yellow for single CTA per surface max

**Reference mood:** Hermes calm + Codex focus + a touch of Resonantia terrain (violet, not red desert).

---

## Critique → phased work

### P0 — Kill the shame (week 1)

*If we only do one sprint, do this.*

| # | Problem | Fix | Touch |
|---|---------|-----|-------|
| P0.1 | Status bar shows `surreal-ws://…`, `rev 190`, `64 tools` | **Operator strip:** `Connected` · `N in motion` · `N blocked` (if >0). Full detail in Settings → Diagnostics drawer | `StatusBar.svelte`, `SettingsPanel.svelte` |
| P0.2 | Activity header repeats daemon URL | Single connection pill in status bar only; Activity header = "Activity" | `ActivityPanel.svelte` |
| P0.3 | Wall of Job Failed / 25 dead letters | **Activity filters:** default `operator` — hide `dead_letter`, dedupe cognitio spam, cap feed at 50. Toggle: "Show all events" | `workspace.svelte.ts`, `ActivityPanel.svelte` |
| P0.4 | Work board counts blocked as "active" on Home | Home hero: **in motion** count separate from **needs attention**; blocked gets its own muted tile with "Review" action | `HomeOverview.svelte` |
| P0.5 | Vault tree shows hash filenames | Display `note.title`; path on hover/tooltip; truncate hash tails (`weekly-review` not `weekly-a51133f…`) | `VaultTree.svelte`, `VaultTreeNode.svelte`, `vaultTree.ts` |
| P0.6 | Chat header shows raw `session_id` | Show `display_name` or first-line preview; id in whisper on hover / Settings diagnostics | `ChatPanel.svelte`, `SessionSidebar.svelte` |
| P0.7 | Context panel shows raw wikilinks | Render as titled links; `[[weekly-review]]` → "Weekly review" | `ContextPanel.svelte`, vault title lookup |
| P0.8 | Developer copy in UI | Copy pass: replace "daemon", "stream", "catalog from the daemon" with operator language | All `*Panel.svelte` headers |

**P0 exit:** Screenshot test — no URL, no hash path, no raw session id in primary UI.

---

### P1 — One hero per surface (week 2)

| # | Problem | Fix | Touch |
|---|---------|-----|-------|
| P1.1 | Home is four stat boxes | **Home v2:** primary card = next action (in-flight card OR last note OR "Start a chat"); secondary row = quiet counts | `HomeOverview.svelte` |
| P1.2 | Chat is three columns cramped | Sessions → **collapsible drawer** (icon toggle); default Chat + Activity only | `WorkshopShell.svelte`, `SessionSidebar.svelte` |
| P1.3 | Work board clipped under Activity | When Work active: widen board — optional **collapse Activity** to icon strip (32px) | `WorkshopShell.svelte`, layout store |
| P1.4 | Unicode nav glyphs | **Lucide icons** via `@lucide/svelte`; consistent 20px stroke | `NavSidebar.svelte` |
| P1.5 | Composer chips `home` / `balanced` meaningless | Replace with readable chips: provider + model from settings/daemon (or hide until Settings wires them) | `ChatPanel.svelte`, settings store |
| P1.6 | Empty states generic | Branded empty copy per surface (README voice); violet accent line, not `MEDOUSA HOME` wireframe | `ChatPanel.svelte`, `HomeOverview.svelte`, `KanbanBoard.svelte` |
| P1.7 | Card titles "Workflow: cognitio" | Map `status_label` + job type to human labels in UI layer (`formatCardTitle`) | `KanbanCard.svelte`, `WorkRail.svelte`, new `formatWork.ts` |

**P1 exit:** Each surface has one obvious focal point at 1280px width.

---

### P2 — Theme + typography system (week 2–3)

| # | Work | Touch |
|---|------|-------|
| P2.1 | Ship **Obsidian** custom Skeleton theme | `medousa-theme.ts`, `tailwind.config.ts`, `app.html` |
| P2.2 | Token audit — 3 surface elevations, 1 primary button style | grep `variant-filled-warning` misuse; standardize |
| P2.3 | Spacing rhythm — 4/8/16/24 only on chrome | layout components |
| P2.4 | Prose in chat — markdown render for assistant (not raw `**`) | `ChatPanel.svelte`, reuse `markdownPreview` |
| P2.5 | Settings appearance section — theme preview swatch | `SettingsPanel.svelte` |

**P2 exit:** Side-by-side screenshot vs current sahara — reads black/violet, not red/maroon.

---

### P3 — Story + docs (week 3)

| # | Work | Touch |
|---|------|-------|
| P3.1 | README section **Medousa Home** — install, run, screenshot | `README.md` |
| P3.2 | Update design doc theme + polish milestones | `medousa-home-tauri-design.md` |
| P3.3 | `medousa doctor` mentions Home connectivity (optional) | daemon CLI |
| P3.4 | First-run empty state when 0 blocked cards (dogfood filter off in dev) | Activity defaults |

---

## Layout rules (post-polish)

```text
┌────┬──────────────────────────────────┬──────────┐
│Nav │  ONE hero surface               │ Activity │  ← collapsible on Work
│    │  (Chat default)                 │ (quiet)  │
├────┴──────────────────────────────────┴──────────┤
│ Work rail — in motion only                        │
├──────────────────────────────────────────────────┤
│ Connected · 2 in motion · 3 need attention       │  ← no URLs
└──────────────────────────────────────────────────┘
```

**Chat:** Nav + Chat + Activity (sessions in drawer)  
**Work:** Nav + Board + Inspector split + Activity collapsed  
**Library:** Nav + Tree + Editor (titles not hashes)

---

## Activity feed — operator semantics

Default feed kinds (show):

- `job_succeeded`, `work_wrapping_up`, `vault_note_*`, `card_*` (non-terminal)
- User-facing summaries only

Hide by default:

- Repeated `job_failed` for same `cognitio` dead_letter
- Terminal blocked cards already on board
- Debug-style duplicate events within 5 minutes

Settings toggle: **Show technical events** (restores current behavior for dev).

---

## Copy voice (README-aligned)

| Don't say | Say |
|-----------|-----|
| daemon returned HTTP 404 | Can't reach Medousa engine |
| Live cards from workspace stream | What's running right now |
| Read-only catalog from the daemon | Your skills and tools |
| Give Medousa a task | What do you want to work on? |
| drag to cancel | Drag here to stop this job |

---

## Milestone label

| Milestone | Scope |
|-----------|-------|
| **M4a** | P0 + Obsidian theme (P2.1) — maximum visual + trust fix |
| **M4b** | P1 layout + icons + Home v2 |
| **M4c** | P3 docs + diagnostics drawer |

---

## Success metrics (dogfood)

1. New user opens Home — does not see a URL in the first 10 seconds
2. Activity feed — at least 1 green/completed event visible before failed (or empty calm state)
3. Library — user recognizes note names without reading hex
4. Screenshot shareable on README without apology
5. Theme reads **black + violet**, not red/gold

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | Initial polish plan from Steve Jobs critique sprint; Obsidian theme direction |
| 2026-05-30 | **M4 shipped:** M4a trust/theme, M4b layout/icons, M4c docs/markdown/doctor |
