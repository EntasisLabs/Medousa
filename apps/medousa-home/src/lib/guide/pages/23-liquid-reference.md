# Liquid blocks

**Advanced.** Liquid blocks are interactive pieces inside notes (cards, charts, feeds, and more). Insert them from the Live slash menu under **Blocks**, or type them in Build.

Related: [Vault and notes](guide:vault-notes) · [Views and environments](guide:views-environments)

## Fence languages

`card` · `carousel` · `actions` (alias `action_row`) · `callout` · `section` · `block` · `chips` (alias `chip_group`) · `media` · `cite` · `compare` · `plan` · `timeline` · `shortlist` · `decision` · `brief` · `dashboard` · `chart` · `report` · `slides` · `tabs` · `steps` · `accordion` · `code` · `tree` · `kanban` · `feed`

**Chart** `type:` values: `bar` · `line` · `area` · `pie` · `donut` · `radar` · `radial` · `scatter` · `combo` · `heatmap`

## Examples

### Callout

````markdown
```callout
tone: note
title: Note
body: Supporting detail for the reader.
```
````

### Chart

````markdown
```chart
type: bar
title: Visitors
legend: bottom

| Category | Desktop | Mobile |
| --- | --- | --- |
| Jan | 186 | 80 |
```
````

### Feed (last-good)

`feed` blocks show the latest good automation output. Empty: “No feed output yet.” Custom views may badge **Live feed** / **Stale feed**.

Prefer configuring living blocks in **Live** (builders/sheets) rather than bouncing to Build to fix a slash mistake.

## Authoring tips

1. Start from slash **Blocks** so the fence skeleton is valid.
2. Nest charts inside `report` when you need a narrative + visuals layout.
3. Keep `feed` ids stable so badges and last-good resolve.
4. Export PDF/Word may flatten some interactivity — check [Vault and notes](guide:vault-notes#export-and-chat-bridges).

Next: [Vault and notes](guide:vault-notes).
