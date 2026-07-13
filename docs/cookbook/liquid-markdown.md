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
- **Optional KV:** `description`, `labels` (`none`|`value`|`category`|`both`), `labelPosition` (`auto`|`inside`|`outside`), `activeKey`, `curve`, `layout`, `stacked`, `centerValue`, `centerLabel`, `tooltip`, `legend`, `separator`, `trend`, `caption`

Prefer ` ```chart ` when a plot communicates better than a raw GFM table; keep plain tables for dumps.

## Vault insert

In the vault editor, type `/` and pick **Chart**, **Card**, **Liquid callout**, or **Dashboard** for a paste-ready fence (edit the numbers in place).
