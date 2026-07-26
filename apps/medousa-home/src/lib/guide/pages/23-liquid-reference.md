# Liquid reference

Liquid fences are interactive document blocks in vault notes (and optionally chat). Insert from the Live slash menu **Blocks** section, or type a fence in Build.

Guide authors: callouts in *this* manual also use ` ```callout ` fences — same syntax, guide pipeline. In the vault, callout is one Liquid kind among many.

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

`feed` blocks hydrate the latest good automation / Stasis output. Empty: “No feed output yet.” Custom views may badge **Live feed** / **Stale feed**.

Prefer configuring living blocks in **Live** (builders/sheets) rather than bouncing to Build to fix a slash mistake.

## Authoring tips

1. Start from slash **Blocks** so the fence skeleton is valid.
2. Nest charts inside `report` when you need a narrative + visuals layout.
3. Keep `feed` ids stable so badges and last-good resolve.
4. Export PDF/Word may flatten some interactivity — check [Vault and notes](guide:vault-notes#export-and-chat-bridges).

Next: [Vault and notes](guide:vault-notes).
