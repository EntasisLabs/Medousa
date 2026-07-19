# Vault slides

**Audience:** operator, writer

16:9 decks in the vault — paste a ` ```slides ` fence into any note, or use a whole-note deck (`kind: slides`). Nested charts, washes, and photo backgrounds are first-class. PDF/Word export is one slide per page.

Related: [Liquid markdown](liquid-markdown.md) (fence catalog) · [Vault & library](vault-and-library.md)

---

## Two surfaces

| Mode | How |
|------|-----|
| **Embed** | ` ```slides ` … ` ``` ` inside a normal note |
| **Deck note** | Frontmatter `kind: slides` + `medousa-deck: basic`; body uses the same grammar (no outer fence) |

Slash insert: `/` → **Slides**. Deck notes open a deck surface (Write for source; column chrome like reports).

---

## Grammar

````md
```slides
title: Mid-2026 pitch
theme: dusk
columns: 2

---
label: Title
layout: hero
bg: ember

# Frontier models
One pick for Live polish

---
label: Price story
layout: split
bg: ./shots/sky.png

Prose wraps beside the chart…

```chart
type: line
title: Price
legend: bottom

| Month | Price |
| ----- | ----- |
| Jan   | 12    |
| Feb   | 18    |
| Mar   | 15    |
```
```
````

### Deck KV (preamble)

| Field | Values | Notes |
|-------|--------|--------|
| `title` | string | Shown above the stage |
| `theme` | `paper` \| `dusk` \| `ink` \| `mist` \| `ember` | Default wash for slides without `bg:` (default `paper`) |
| `columns` | `1` \| `2` \| `3` | Figure columns for `layout: split` (default `2`) |

### Per-slide KV

| Field | Values | Notes |
|-------|--------|--------|
| `label` | string | Tab label |
| `layout` | `hero` \| `split` \| `stack` | Hero = title-centric; **split** = prose beside figures; stack = vertical |
| `bg` | wash id **or** image path/URL | Wash: `paper`/`dusk`/`ink`/`mist`/`ember`. Image: `./shots/sky.png`, `/…`, or `https://…` |
| `scrim` | `none` \| `dark` \| `light` | Overlay on **image** backgrounds only. Default **`none`** (full photo + **dark** ink). Use `dark` for a dim overlay + **light** ink on busy/dark photos; `light` for a cream wash + dark ink |

Section body is nest-aware markdown — nested ` ```chart `, ` ```media `, callouts, etc. hydrate like reports.

---

## Atmosphere (washes + photos)

**Washes** are preset CSS gradients (not freeform CSS):

| id | Feel | Ink |
|----|------|-----|
| `paper` | Cream | Dark |
| `mist` | Soft cool gray | Dark |
| `dusk` | Deep blue-gray | Light |
| `ink` | Near-black | Light |
| `ember` | Charcoal → rust | Light |

**Photos:** set `bg:` to a vault-relative path (same resolution as `![](./…)`). Default is no darken (`scrim: none`) with dark text — best for light/graphic backgrounds. For dark photography, add a scrim so light ink stays readable:

```markdown
bg: ./shots/hero.png
scrim: dark
```

Deck `theme:` is the fallback when a slide omits `bg:`.

---

## Content images (not backgrounds)

Use either markdown images or a ` ```media ` fence inside the slide body — both resolve vault-relative paths (same as `![](./…)`):

````md
```media
src: ./shots/architecture.svg
alt: System diagram
caption: Mid-2026 topology
ratio: 16/9
```
````

Or: `![](./shots/logo.png)` / `![](./shots/logo.png|280)` (Obsidian-style size). Keep `bg:` for full-bleed atmosphere only.

## Layout tips

- **`split` + `columns: 2`:** a paragraph and a chart/media sit side by side. Headings still span full width.
- **`stack`:** single column, top-down reading flow.
- **`hero`:** title slide — label kicker + large type anchored bottom-left in the 16:9 frame (empty stage to the right is intentional).
- Prefer one figure per split row for pitch decks; use `stack` for long prose.

---

## Whole-note deck

```yaml
---
kind: slides
medousa-deck: basic
author: Ada
date: 2026-07-19
---
```

Body = same KV + `---` sections as the fence (no wrapping ` ```slides `). Export wraps the body automatically.

Optional export byline: frontmatter `author` / `date` + Export sheet toggles **Include author** / **Include date**.

---

## Export

- **PDF:** each slide is a page (`vault-export-page-break` between frames); washes and photos bake into the frame.
- **Word:** each `.liquid-slide` is snapshotted as an image (same path as other liquid embeds).

Out of scope for v1: presenter mode, freeform CSS gradients, LME HTML artifact decks (see [Artifacts & presentations](artifacts-and-presentations.md)).
