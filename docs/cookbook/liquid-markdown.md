# Liquid markdown embeds

Paste-first UI blocks for **chat** and **vault** notes. The client turns fenced blocks into Liquid components (cards, charts, dashboards, …) — do not invent HTML/CSS.

## Surfaces

| Surface | Behavior |
|---------|----------|
| Chat | Hydrated via `MarkdownContent` |
| Vault preview | Same hydrate pipeline (charts, cards, Mermaid, code) |
| Vault slash (`/`) | Insert starters: Chart, Card, Liquid callout, Dashboard |
| PDF export | Hydrates then captures (charts render as painted DOM) |

Live gallery (dev): `/dev/liquid` in medousa-home.

## Fence catalog

Supported langs: `card`, `carousel`, `actions`, `callout`, `section`, `chips`, `media`, `cite`, `compare`, `plan`, `timeline`, `shortlist`, `decision`, `brief`, `dashboard`, `chart`, plus `mermaid` and `{{icon:name}}`.

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

- **type:** `bar` | `line` | `area` | `pie` | `donut` | `radar` | `radial`
- **Table:** first column = categories (radar axes); other columns = numeric series
- **Minimum rows:** ≥2 categories (≥3 for radar)
- **Optional KV:** `description`, `labels` (`none`|`value`|`category`|`both`), `labelPosition` (`auto`|`inside`|`outside`), `activeKey`, `curve`, `layout`, `stacked`, `centerValue`, `centerLabel`, `tooltip`, `legend`, `separator`, `trend`, `caption`, `colors`, `width` (`sm`|`md`|`lg`|`full`|CSS length), `height` (`sm`|`md`|`lg`|CSS length, bar/line/area), `surface` (drawing wash — see below)
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

## Vault insert

In the vault editor, type `/` and pick **Chart**, **Card**, **Liquid callout**, or **Dashboard** for a paste-ready fence (edit the numbers in place).
