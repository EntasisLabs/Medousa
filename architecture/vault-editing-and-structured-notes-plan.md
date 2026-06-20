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
| 5 | **Polish writes grep-able markdown** — no hidden UI-only state (completion stamps inline; embeds are file paths) |
| 6 | **Graph view not in Library** — Context map covers relationship discovery |
| 7 | **No Notion-style properties panel** — optional frontmatter keys only when needed; no schema UI |

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

## Phase G — Friction, environment & daily-driver polish

**Goal:** Reduce friction so the vault is a place to **manage everything** without Medousa-specific lock-in. UI is a lens; files on disk stay portable (Obsidian, vim, git).

**North-star test:** If Home vanished tomorrow, notes remain fully usable elsewhere; every polish feature writes **grep-able markdown** or copies **filesystem paths**.

### Locked decisions (2026-06)

| Topic | Decision |
|-------|----------|
| **Note workshop** | Floating **modal** (draggable + minimizable). Keep **Ask about note** entry. Same daemon/session as Chat — not a second tier. Tone: helpful companion, not corporate “Help Bot”. |
| **Inline images** | `![alt](path)` for **relative or absolute** local paths when readable; `https://` unchanged. Attachment/image **context menu → Insert embed** inserts markdown at cursor. |
| **Task completion date** | **Inline text** when enabled (setting; off by default). Example: `- [x] Ship feature (done 2026-06-07)`. |
| **Context menu mobile** | **Long-press** parity with desktop right-click. |
| **Ship order** | **Highest ROI → lowest** (G1 → G2 → G3 → G4 → G5 below). |
| **Transclusion** | Yes — `![[note]]` / embed another note in preview; source stays separate files. |
| **Properties UI** | No dedicated Notion-style panel. |
| **Graph in Library** | No — Context view already covers this. |
| **User templates** | Save **custom templates** (markdown snippets user defines); appear in New note alongside built-ins. |

### G1 — Context & filesystem honesty *(highest ROI)*

**Goal:** Finder/Obsidian muscle memory on tree, preview, attachments.

| Item | Work |
|------|------|
| G1a | Shared context menu: tree row, preview, attachment chip |
| G1b | **Copy path**, **Copy wikilink** `[[…]]` |
| G1c | **Rename**, **Move**, **Duplicate note** (surface existing actions) |
| G1d | **Reveal in Finder**, **Open with default app** |
| G1e | **Copy as markdown** (full note body) |
| G1f | Desktop: right-click · Mobile: **long-press** |

**Acceptance:** Right-click daily note → copy path → paste into `from:` or agent prompt; long-press same on phone.

### G2 — Preview as workspace *(high ROI)*

**Goal:** Interact in preview; source updates on disk.

| Item | Work |
|------|------|
| G2a | Toggle `- [ ]` / `- [x]` in preview (writes markdown + autosave) |
| G2b | Setting: **stamp completion inline** when checking (default off) |
| G2c | Light preview actions: jump to source line (optional), find-in-note ⌘F |

**Acceptance:** Preview checklist on daily note; checked items show optional inline date when setting on.

### G3 — Command surfaces *(medium-high ROI)*

**Goal:** One universal friction reducer without new schema.

| Item | Work |
|------|------|
| G3a | Slash expansion: **link picker** → `[[path\|label]]` |
| G3b | Slash: `/view`, `/board`, `/table`, `/toc` templates |
| G3c | **Quick switcher** ⌘O — fuzzy open note |
| G3d | **User templates** — save, name, delete; stored locally; show in New note dialog |
| G3e | **Transclusion** — `![[note]]` renders embedded note body in preview (file unchanged) |
| G3f | Phase B polish: compact format bar / fold behind **Format** on small screens |

**Acceptance:** Type `/` → pick vault note → wikilink inserted; save “Weekly retro” template → reuse from New note.

### G4 — Inline richness *(medium ROI)*

**Goal:** Notes with images feel complete; still standard markdown.

| Item | Work |
|------|------|
| G4a | Preview renders `![alt](relative-or-absolute-path)` when file readable |
| G4b | Resolve paths relative to note directory + vault root heuristics |
| G4c | Attachment bar / file context: **Insert embed** → `![](path)` at cursor |
| G4d | Optional: CSV copy from medousa-view table |

**Acceptance:** Drop `![](../screenshots/foo.png)` in note → preview shows image; right-click attachment → embed inserted in source.

### G5 — Note workshop modal *(transformative, larger UX)*

**Goal:** Scoped collaboration on the current note without leaving Library.

| Item | Work |
|------|------|
| G5a | Floating panel: drag, minimize, restore; opened from **Ask about note** (keep tab fallback) |
| G5b | Scoped thread: this note + backlinks + vault search bias |
| G5c | Web search + suggest edits; same turn/stream infra as Chat |
| G5d | Outcomes land in **markdown** (agent proposals use existing proposal bar) |

**Not v1:** Separate memory graph, separate model defaults, export-only-in-Medousa format.

**Acceptance:** Read note → Ask → floating panel → ask “what links here?” → answer cites vault paths; minimize → keep reading note.

### Also in polish backlog (lower urgency)

| Item | Notes |
|------|-------|
| Backlinks panel on mobile | Desktop has links panel; expose in reader |
| Word count / status whisper | Trivial status bar |
| PDF export in context menu | Already exists; surface it |
| Paste image → vault subfolder | Obsidian-style; defer until G4 stable |

### Implementation order

```text
G1 (context) → G2 (preview todos) → G3 (slash/switcher/templates/transclusion) → G4 (images) → G5 (workshop modal)
```

B4 (mobile format bar) ships inside **G3f**.

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
A → B → C → D → E → F → G

G: context (G1) → preview todos (G2) → slash/switcher/templates (G3) → images (G4) → workshop modal (G5)
```

---

## Checklist

- [x] Phase A complete (nav persistence, move filter, ⌘A)
- [x] Phase B core (mobile preview→edit, header chrome, note actions)
- [ ] Phase B polish (compact format bar — tracked as G3f)
- [x] Phase C complete (heading ids, wikilink+heading scroll, unresolved affordance, TOC block)
- [x] Phase D complete (mobile talk-about-note, proposal diff UX, scoped context chip)
- [x] Phase E spec frozen + shipped (markdown kanban board editor)
- [x] Phase F spec frozen + shipped (medousa-view query blocks)
- [x] Phase G1 — context menu (copy path/wikilink, reveal, duplicate, long-press)
- [x] Phase G2 — preview checkboxes + optional inline completion stamp
- [ ] Phase G3 — slash expansion, quick switcher, user templates, transclusion, mobile format bar
- [ ] Phase G4 — local image embeds + insert embed from attachments
- [ ] Phase G5 — floating note workshop modal (Ask about note)

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
