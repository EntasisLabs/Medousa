# Liquid markdown embeds

Paste-first UI blocks for **chat** and **vault** notes. The client turns fenced blocks into Liquid components (cards, charts, dashboards, …) — do not invent HTML/CSS.

## Surfaces

| Surface | Behavior |
|---------|----------|
| Chat | Hydrated via `MarkdownContent` |
| Vault preview | Same hydrate pipeline (charts, cards, Mermaid, code) |
| Vault slash (`/`) | Insert starters: Chart, Card, Liquid callout, Dashboard, Report, Tabs, Steps, Accordion, Code snippet, File tree |
| PDF export | Hydrates then captures (charts render as painted DOM) |

Live gallery (dev): `/dev/liquid` in medousa-home.

## Fence catalog

Supported langs: `card`, `carousel`, `actions`, `callout`, `section`, `chips`, `media`, `cite`, `compare`, `plan`, `timeline`, `shortlist`, `decision`, `brief`, `dashboard`, `chart`, `report`, `tabs`, `steps`, `accordion`, `code`, `tree`, plus `mermaid` and `{{icon:name}}`.

Agents on UI-capable clients get recipes from `[MEDOUSA_PRESENTATION]` and `cognition_environment_wiki(topic=ui_scene|scene_vs_html)`.

## Chart schema

````md
```chart
type: bar
title: Visitors
legend: bottom

| Month | Desktop | Mobile |
| ----- | ------- | ------ |
| Jan   | 186     | 80     |
| Feb   | 305     | 200    |
```
````

- **type:** `bar` | `line` | `area` | `pie` | `donut` | `radar` | `radial` | `scatter` | `combo` | `heatmap`
- **Category table** (`bar` / `line` / `area` / `pie` / `donut` / `radar` / `radial` / `combo`): first column = categories (radar axes); other columns = numeric series
- **Scatter table:** Col1 = X, Col2 = Y, optional Col3 = group/series key
- **Heatmap table:** header row = column labels (first cell empty/corner); first column = row labels; cells = numbers
- **Minimum rows:** ≥2 categories (≥3 for radar); scatter ≥2 points; heatmap ≥1 data row
- **Optional KV:** `description`, `labels` (`none`|`value`|`category`|`both`), `labelPosition` (`auto`|`inside`|`outside`), `activeKey`, `curve`, `layout`, `stacked`, `centerValue`, `centerLabel`, `tooltip`, `legend`, `separator`, `trend`, `caption`, `colors`, `width` (`sm`|`md`|`lg`|`full`|CSS length), `height` (`sm`|`md`|`lg`|CSS length, bar/line/area/combo/scatter), `surface` (drawing wash — see below), `seriesMarks` (`bar, line` — combo only; default first series bar, rest line)
- **Combo note:** `seriesMarks` assign each series to bar (left Y) or line (right Y). Mixed units (revenue + growth %) render correctly without pre-normalizing.
- **Colors:** defaults are readable blues/purples/greens (not ink-black). Override with markdown color names or hex:

````md
```chart
type: bar
colors: blue, purple, green
legend: bottom
width: md

| Month | Desktop | Mobile | Tablet |
| ----- | ------- | ------ | ------ |
| Jan   | 186     | 80     | 120    |
| Feb   | 305     | 200    | 90     |
```
````

**Surface** (radar / donut / pie / radial plot wash — not the card chrome):

Zero-config is finished: omit `surface` on radial/pie (no plate — marks float on the card). Radar keeps a whisper-soft default plate; use `surface: none` to drop it.

| Value | Effect |
| --- | --- |
| _(omit)_ / `soft` | Radial/pie: no plate. Radar: very light theme wash |
| `muted` | A bit more contrast |
| `none` | No plate |
| `gray` / `grey` | Soft neutral wash (~12%) |
| `blue`, `pink`, … | Tinted wash (~16%) |
| `blue/25` or `gray/40` | Hue + opacity (0–1 or 0–100%) |
| `#94a3b8/30` | Hex wash + opacity |

Prefer `gray/10`–`/20` or `blue/15` when you want a visible plate without a heavy disk.

````md
```chart
type: radar
title: Team coverage
surface: gray/12
colors: blue, green
legend: bottom

| Axis        | Alpha | Beta |
| ----------- | ----- | ---- |
| Speed       | 80    | 55   |
| Reliability | 70    | 85   |
| Comfort     | 60    | 70   |
| Safety      | 90    | 65   |
| Efficiency  | 75    | 90   |
```
````

Named colors match `{{blue|text}}`: `red`, `orange`, `yellow`, `green`, `blue`, `purple`, `pink`. Hex works too (`#2563eb`).

Value labels are opt-in (`labels: value` / `both`). Titles, axes, and legends use high-contrast chart type tokens so they stay readable on cream and dark. Bars use pill leading edges; lines include a soft wash under the stroke.

Prefer ` ```chart ` when a plot communicates better than a raw GFM table; keep plain tables for dumps.

## Report schema

Narrative + figures — not a KPI dashboard. Use ` ```report ` when prose and charts belong together; use ` ```dashboard ` for at-a-glance tiles; use bare ` ```chart ` for a single plot.

````md
```report
title: Q2 growth review
subtitle: North America · weekly pulse
columns: 2

Opening prose stays full-bleed.

```chart
type: combo
title: Revenue vs growth
legend: bottom
seriesMarks: bar, line

| Month | Revenue | Growth % |
| ----- | ------- | -------- |
| Jan   | 120     | 4        |
| Feb   | 148     | 7        |
```

```chart
type: heatmap
title: Engagement matrix
colors: blue

|           | Mon | Tue | Wed |
| --------- | --- | --- | --- |
| Morning   | 2   | 5   | 3   |
| Afternoon | 8   | 6   | 9   |
```

## Deep dive

More prose after the figures.
```
````

- **KV:** `title`, `subtitle`, `columns` (`1`|`2`|`3`, default `2`)
- **Body:** markdown after the preamble — nested ` ```chart ` fences hydrate innermost-first; prose spans full width, consecutive chart embeds sit in the column grid

## Tabs / steps / accordion

Multi-panel and procedural blocks share the same paste shape: optional header KV, then `---` separated sections with `label:` + `body:`.

````md
```tabs
title: Getting started
default: Run

---
label: Install
body: npm install medousa
---
label: Run
body: medousa up
```
````

- **tabs:** ≥2 panels; optional `default:` (label, id, or 1-based index)
- **steps:** ≥2 steps; optional per-step `status:` `done`|`current`|`pending`
- **accordion:** ≥1 item; optional `multiple: true`, per-item `open: true`

````md
```steps
title: Ship it

---
label: Build
body: cargo build --release
status: done
---
label: Deploy
body: Push to production
status: current
```
````

## Code snippet

Enhanced fence (lang badge + copy). Requires liquid chrome (`lang:` / `title:` / `---` + source) so mistaken prose ` ```code ` still unwraps.

````md
```code
lang: typescript
title: greet.ts
---
export function greet(name: string) {
  return `Hello, ${name}`;
}
```
````

- Optional `diff: true` (or `lang: diff`) tints `+` / `-` lines
- Optional `copy: false` hides the copy button

## File tree

Indented list (2 spaces). Trailing `/` marks folders.

````md
```tree
title: Project
---
src/
  lib/
    index.ts
README.md
```
````

## Vault insert

In the vault editor, type `/` and pick **Chart**, **Card**, **Liquid callout**, **Dashboard**, **Report**, **Tabs**, **Steps**, **Accordion**, **Code snippet**, or **File tree** for a paste-ready fence.
