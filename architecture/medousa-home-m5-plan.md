# Medousa Home — M5 Plan (World Class)

> **Status:** M5b shipped — M5c pending  
> **Date:** 2026-05-30  
> **Related:** [medousa-home-polish-plan.md](medousa-home-polish-plan.md), [medousa-home-tauri-design.md](medousa-home-tauri-design.md)

## North star

M4 made Medousa Home **honest and calm**. M5 makes it **inevitable** — the kind of desktop app you leave open all day without apologizing for what it shows.

**Mantra:** One next thing. No duplicate shame. No plumbing in primary UI.

---

## Audit → milestone map

| Audit finding | M5 phase | Exit criterion |
|---------------|----------|----------------|
| Hero shows `Daily 6834c1a…` | **M5a P0.1** | No hex ≥8 chars in any primary title |
| Vault tree: three identical "Daily" | **M5a P0.1** | Duplicate labels disambiguated |
| Work rail empty sermon on every surface | **M5a P0.2** | Rail hidden unless in-motion or Work surface |
| 25 identical blocked cards | **M5a P0.3** | Grouped + capped column; batch entry |
| Editor shows raw path + raw wikilinks | **M5a P0.4** | Breadcrumb title; humanized preview links |
| Home promotes note when 25 blocked | **M5a P0.5** | Hero = review blocked when attention > 0 |
| Kanban neon column frames | **M5b P1.1** | One border language |
| Skills registry UI | **M5b P1.2** | Consumer cards; technical in disclosure |
| Duplicate status counts | **M5b P1.3** | Single operator strip |
| Nav / chrome polish | **M5b P1.4** | Icon-only actions; nav brand cleanup |
| Screenshot / dogfood | **M5c P2** | README shot; blocked cleanup CTA |

---

## M5a — Stop the embarrassment (P0)

### P0.1 — Title sanitizer v2

**Problem:** `formatVault` strips hash *filename tails* but passes through titles like `Daily 6834c1a…`.

**Fix:**

- `stripEmbeddedHashes()` on all display paths (title, wikilink, preview)
- `vaultTreeDisambiguator()` — when sibling leaves share display title, append short date or hash prefix (`Daily · Mar 5`, `Daily · …e798`)
- Apply in: `VaultTreeNode`, `HomeOverview`, `VaultEditor`, `markdownPreview` wikilinks

**Touch:** `formatVault.ts`, `VaultTreeNode.svelte`, `VaultEditor.svelte`, `markdownPreview.ts`

### P0.2 — Contextual work rail

**Problem:** 112px footer on Home/Chat/Library preaches "no in-motion work" while status bar says 25 need attention.

**Fix:**

- Show `WorkRail` only when `inMotionCount > 0` **or** `activeSurface === 'work'`
- When hidden, status bar remains sole operator strip

**Touch:** `WorkshopShell.svelte`, optionally `WorkRail.svelte`

### P0.3 — Blocked column grouping

**Problem:** 25× "Stuck job needs review" — cemetery, not board.

**Fix:**

- `groupBlockedCards()` — cluster by `formatCardTitle` + `status_label`
- Kanban blocked column: show group header + count; expand or "Review N similar"
- Cap rendered cards at 8 with "+N more" row
- Optional: single "Review all blocked" affordance in column header

**Touch:** new `groupWork.ts`, `KanbanBoard.svelte`, `KanbanCard.svelte`

### P0.4 — Library editor header

**Problem:** Raw path under title; preview wikilinks show UUID slugs.

**Fix:**

- Header: breadcrumb (`Journal › Daily`) + human title only
- Path in `title` attribute / whisper on hover — not visible by default
- `markdownPreview`: resolve wikilinks via vault title map when rendering preview

**Touch:** `VaultEditor.svelte`, `formatVault.ts`, `markdownPreview.ts`, `vault.svelte.ts`

### P0.5 — Home hero priority

**Problem:** Last note wins hero when blocked work needs attention.

**Fix:**

Priority order:

1. In-motion card (unchanged)
2. **Needs attention** (`blockedCount > 0`) → "Review blocked work" hero
3. Last note
4. Start a chat

**Touch:** `HomeOverview.svelte`

---

## M5b — Feel expensive (P1)

| # | Work | Touch |
|---|------|-------|
| P1.1 | Kanban: subtle column headers, remove per-column neon borders | `kanban.ts`, `KanbanBoard.svelte` |
| P1.2 | Skills: consumer card layout; bindings/manuscript ids in disclosure | `SkillsPanel.svelte` |
| P1.3 | Remove duplicate attention counts from Home card when status bar shows same | `HomeOverview.svelte` |
| P1.4 | Nav: drop perpetual "Home" subtitle; surface title in main header only | `NavSidebar.svelte` |
| P1.5 | Ghost icon buttons: Pop out, collapse, sessions — consistent 20px | layout components |

---

## M5c — Screenshot-ready (P2)

| # | Work |
|---|------|
| P2.1 | README screenshot (blocked grouped or filtered) |
| P2.2 | First-run: if `blocked > 10`, Home hero emphasizes batch review + Settings link to technical events |
| P2.3 | Design doc M5 section + polish plan cross-link |

---

## Success metrics

1. Screenshot test — no hex string ≥8 chars in primary UI at 1280×800
2. Home with 25 blocked — hero says **Review**, not **Daily …e798**
3. Chat/Library — no empty work rail
4. Blocked column — ≤8 visible rows + grouped summary
5. Shareable README image without apology

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | M5 plan from Steve Jobs audit round 2 |
| 2026-05-30 | **M5a shipped:** P0.1–P0.5 (sanitizer, rail, blocked groups, editor, hero) |
| 2026-05-30 | **M5b shipped:** P1.1–P1.5 (kanban tones, Skills cards, nav, icon buttons) |
