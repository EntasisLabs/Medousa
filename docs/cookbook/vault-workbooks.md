# Vault workbooks

Multi-sheet workbooks with Excel-style formulas. Computed values are **overlay-only** — they show in Live/Build and are not written into the markdown table.

## Folder layout

```text
vault/workbooks/Q3-budget/
  _workbook.md
  .medousa-workbook          # optional machine marker
  Revenue.md
  Costs.md
  Summary.md
```

### `_workbook.md`

```yaml
---
kind: workbook
title: Q3 budget
sheets:
  - Revenue
  - Costs
  - Summary
---
```

### Sheet note

```yaml
---
kind: sheet
workbook: Q3-budget
sheet: Revenue
formulas:
  B3: "=SUM(B2:B2)"
  C3: "=Costs!B2*1.1"
---

| Item   | Amount |
| ------ | ------ |
| Widget | 40     |
```

Formulas live only under `formulas:`. Pipe cells stay literals or empty.

## Engine

HyperFormula (GPL-v3 OSS key) evaluates the workbook folder as a multi-sheet session. Cross-sheet refs use the stem names from `sheets:`.

## Agent note

Grep and MCP vault reads see markdown literals, not overlay totals.
