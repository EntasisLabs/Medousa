# Vault and notes

The vault is the workshop’s library on disk — notes, boards, sheets, and richer artifacts. Browse it from **Library** (Notes / Local Files / Presentations). Recovery lives in [Vault trash and versions](guide:vault-recovery). Interactive fences: [Liquid reference](guide:liquid-reference).

## Library chrome

| Mode | For |
|------|-----|
| **Notes** | Vault tree — Recent, Folders, Tags, Kind |
| **Local Files** | **Your files** — pinned host folders (**+ Pin folder**) |
| **Presentations** | Deck artifacts |

Pinned folders stay on this Mac; remote workshops show that Your files are local-only. Prefer vault paths for anything the agent should own.

## Live, Build, and Preview

| Plane | Role |
|-------|------|
| **Live** | Default WYSIWYG — slash menu, selection formatting, living Liquid blocks |
| **Build** | Raw markdown — format bar, split preview, **Links** panel |
| **Preview** | Read-only rendered note |

Switch via overflow (**Edit source** / **Back to Live**, often ⇧E). **Live** is the normal editing view; **Build** shows the raw text when you need it.

Autosave runs on a short debounce (about 4.5s) when enabled; header shows **Saving…** / **Saved**. Manual **Save now** (⇧S) is available from Build habits. Autosave pauses during conflicts, agent proposals, or slash composition.

## Slash menu (`/`)

**Writing:** Link to note, headings, lists, to-do, web link, quote, divider.  
**Blocks:** Callout, Card, charts, Query view, Kanban, Data table, Embed note, Feed, Report, Slides, and more — full fence list in [Liquid reference](guide:liquid-reference).

## Links and embeds

| Feature | How |
|---------|-----|
| Wikilink | `[[path\|label]]` or slash **Link to note** |
| Embed | `![[path]]` or slash **Embed note** — editable through in Live |
| Backlinks | Build **Links** panel — **Out** / **Back** |

## Structured notes

Toolbar toggles when the note kind supports them:

| Kind | Views |
|------|--------|
| Kanban | **Board view** ↔ raw markdown |
| Sheet / ledger | **Table view** ↔ raw |
| Workbook | **Workbook view** ↔ raw |
| Slides | **Deck view** ↔ raw |
| Query view | Live table from other notes — slash **Query view** |

## Export and chat bridges

| Action | Where |
|--------|--------|
| **Export PDF…** / **Export Word…** / **HTML** / **Markdown** | Overflow (⇧P for PDF) → preview → save |
| **Talk about this note** | Overflow / context menu → grounds chat |
| **Send to Work** | Same menus → Work card |

Disabled for **Loose file** notes outside the vault. HTML export is self-contained (Liquid frozen where needed); Markdown export keeps the clean note plus fences.

## Proofread and tables

| Feature | How |
|---------|-----|
| **Grammar check** | Settings → Preferences → **Notes proofread** — off by default; underlines suggestions in Build with fixes you can accept. LanguageTool-compatible endpoint; only note text is sent, never paths |
| **Paste CSV / Excel** | Copy from a spreadsheet and paste straight into a **Data table** or a chart's data editor — or use the table's **Import** for `.xlsx` / `.csv` / `.tsv` |
| **Chart export** | Chart overflow → **PNG**, **SVG**, or **Copy CSV** of the underlying table |

## Agent updates and conflicts

| Banner | Actions |
|--------|---------|
| **Agent updated this note** / changed elsewhere (proposal) | **Keep editing**, **Take agent version**, **Keep mine** |
| **This note changed elsewhere while you were editing** (save conflict) | **Reload**, **Keep mine**, **History** (if Versions on) |

Details: [Vault trash and versions](guide:vault-recovery).

## Sticky note window

Pop a note into the **vault sticky** when you want it beside another app. Close hides; summon again from toolbar / Spotlight.

```callout
tone: tip
title: Write for retrieval
body: Short titles, clear first lines, and stable paths beat clever naming. The agent searches what you leave it.
```

Next: [Vault trash and versions](guide:vault-recovery) · [Liquid reference](guide:liquid-reference).
