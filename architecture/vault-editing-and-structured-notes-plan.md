# Vault editing & structured notes — plan

Living plan for **Library** as the “Obsidian × Notion, AI-native” notebook: solid editing, navigation, markdown-native structure (kanban + database views), without a parallel data model.

**Home app:** `apps/medousa-home` · **Store:** `vault.svelte.ts` · **Mobile:** `MobileLibraryPanel.svelte`, `YouHub.svelte`

**Related:** [polish-and-package-plan.md](polish-and-package-plan.md), [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md)

---

## Product stance

| Principle | Meaning |
|-----------|---------|
| **Markdown is source of truth** | UI views read/write the same files the agent sees. |
| **Work ≠ Vault** | Work kanban = live agent jobs. Vault kanban = personal boards in a note. |
| **Preview first on mobile** | Reader default; explicit **Edit** for markdown source. |
| **No schema engine** | “Databases” = tables + query blocks, not Notion-complete. |

---

## Locked decisions

| # | Decision |
|---|----------|
| 1 | Mobile: **preview first**, tap **Edit** for markdown source |
| 2 | Drag-move filter: **stay on All only if All was active before the drop**; otherwise focus destination space |
| 3 | Links: Obsidian-style `[[wikilinks]]` + Notion-smooth in-doc navigation; AI reads/writes same files |
| 4 | **⌘A / Ctrl+A** in **markdown source textarea only** |

---

## Phase A — Navigation & editor basics ✅ (in progress)

**Goal:** Library feels like home base; desktop markdown feels like an editor.

| Item | Work |
|------|------|
| A1 | Persist `youDestination` + `libraryView` (list \| reader) + list scroll in layout/localStorage |
| A2 | Stop resetting You → hub on tab return (`switchMobileTab`) |
| A3 | Remove “reset to list” when Library panel hides |
| A4 | Move filter: `focusSpaceForPath` only when `activeSpaceFilter !== null` before drop |
| A5 | `VaultMarkdownEditor`: explicit select-all on ⌘A / Ctrl+A |

**Acceptance:** Open note on mobile → Chat → You → still in note reader; drag 3 files with All filter unchanged; ⌘A selects all in source.

---

## Phase B — Mobile editing

**Goal:** Phone can edit notes without split view.

| Item | Work |
|------|------|
| B1 | `VaultEditor`: allow markdown editor on mobile when `editorMode === "edit"` (no split) |
| B2 | Mobile reader header: **Edit / Preview**, **⋯** (rename/move/delete), save whisper |
| B3 | Open note on mobile → `enterPreviewMode()` |
| B4 | Compact format bar or fold behind “Format” on small screens |

**Acceptance:** Daily note: read → Edit → change text → autosave → Preview.

---

## Phase C — Link graph (Obsidian + Notion navigation)

**Goal:** Links go somewhere; in-doc jumps feel smooth.

| Item | Work |
|------|------|
| C1 | Stable `id` slugs on rendered headings |
| C2 | `[[note#Heading]]` → open note + scroll to heading |
| C3 | Unresolved wikilink: gentle affordance (stub / pick note) |
| C4 | Optional auto-TOC block from headings — ` ```medousa-toc``` ` |
| C5 | Wikilink styling + mobile tap feedback |

**Acceptance:** `[[projects#Tasks]]` lands on the right section; external `[text](url)` opens in new tab.

---

## Phase D — AI-native notebook loops

| Item | Work |
|------|------|
| D1 | Mobile “Talk about this note” from reader |
| D2 | Proposal accept/diff UX on mobile |
| D3 | Scoped context hint when agent uses a note (“this page + links”) |

---

## Phase E — Vault kanban (markdown-native)

**Pattern:** Same as `LedgerTableEditor` — parse region → UI → write back.

**Storage convention (v1):**

```markdown
---
medousa-board: basic
---

## Backlog
- [ ] Draft blog post
- [ ] [[Research competitor notes]]

## Doing
- [x] Fix mobile nav

## Done
- [x] Ship Phase A
```

| Item | Work |
|------|------|
| E1 | `findKanbanBoard` / `replaceKanbanBoard` in `markdownKanban.ts` |
| E2 | `KanbanBoardEditor.svelte` — columns = `##`, cards = list items, drag between columns |
| E3 | Vault kind or `boardEditMode: board \| raw` toggle (mirror ledger) |
| E4 | Card body supports wikilinks; open linked note on tap |

**Not v1:** assignees, due dates, sync with Work board.

---

## Phase F — Vault database views (markdown-native)

**Pattern:** Source file holds the table; view file embeds a query block.

**Source (database file):**

```markdown
# Projects

| name | status | due |
|------|--------|-----|
| Mobile nav | done | 2026-06-01 |
| Wikilinks | doing | 2026-06-15 |
```

**View file:**

````markdown
# Active tasks

```medousa-view
from: projects/index.md
table: first
where: status != done
sort: due
columns: name, status, due
```
````

| Item | Work |
|------|------|
| F1 | Parse `medousa-view` fenced blocks in preview |
| F2 | Resolve `from` path, `findLedgerTable` / first GFM table |
| F3 | Filter/sort rows (simple equality / `!=`) |
| F4 | Render live table in preview; link “Edit source” |
| F5 | Agent can read/write source + view blocks |

**Not v1:** multi-file joins, rollups, relation columns, formula engine.

---

## Work vs Vault kanban

| Surface | Purpose | Storage |
|---------|---------|---------|
| **Work** | Agent jobs in motion | Workspace / daemon |
| **Vault board** | Personal / project planning | Markdown note |
| **Vault view** | Dashboard over a table | Query block + source note |

---

## Implementation order

```text
A (nav + filter + ⌘A) → B (mobile edit) → C (links) → D (AI polish) → E (kanban) → F (views)
```

---

## Checklist

- [x] Phase A complete (nav persistence, move filter, ⌘A)
- [x] Phase B core (mobile preview→edit, header chrome, note actions)
- [ ] Phase B polish (compact format bar on small screens)
- [x] Phase C complete (heading ids, wikilink+heading scroll, unresolved affordance, TOC block)
- [x] Phase D complete (mobile talk-about-note, proposal diff UX, scoped context chip)
- [x] Phase E spec frozen + shipped (markdown kanban board editor)
- [x] Phase F spec frozen + shipped (medousa-view query blocks)

---

## Code anchors

| Path | Role |
|------|------|
| `stores/layout.svelte.ts` | Mobile You/Library navigation state |
| `mobileNavigation.ts` | Tab switch behavior |
| `components/mobile/MobileLibraryPanel.svelte` | Mobile notes list + reader |
| `components/vault/VaultEditor.svelte` | Edit/preview chrome |
| `utils/vaultNoteBridge.ts` | Vault ↔ chat bridge, scoped context |
| `components/vault/VaultChatContextChip.svelte` | Chat composer note scope chip |
| `components/vault/VaultProposalBar.svelte` | Agent/server proposal merge UX |
| `components/vault/VaultMarkdownEditor.svelte` | Source editor |
| `utils/markdownKanban.ts` | Kanban board parse/serialize |
| `components/vault/KanbanBoardEditor.svelte` | Board view editor |
| `components/vault/LedgerTableEditor.svelte` | Precedent for structured markdown UI |
| `utils/markdownView.ts` | View block parse/query/render |
| `utils/resolveMedousaViews.ts` | Async source note fetch for views |
| `utils/markdownTable.ts` | Table parse/replace |
| `markdown/preprocess.ts` | Wikilink preprocessing |
| `utils/resolveWikilink.ts` | Client wikilink resolution |
