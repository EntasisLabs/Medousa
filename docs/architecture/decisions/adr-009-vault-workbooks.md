# ADR-009: Vault workbooks + overlay formulas

## Status

Accepted

## Context

Ledgers are single GFM tables. Users need Excel-like multi-sheet workbooks with formulas, without turning markdown into a proprietary binary or rewriting computed values into pipe cells (which would poison git diffs and agent grep).

## Decision

1. **Explicit workbook marker:** a folder is a workbook only when it contains `_workbook.md` (`kind: workbook` + ordered `sheets:`) and/or `.medousa-workbook`.
2. **Sheet notes:** `kind: sheet` with a single GFM pipe table in the body.
3. **Formulas in frontmatter only:** `formulas:` map of A1 → `=…`. Table cells stay literals/empties.
4. **Engine:** HyperFormula (`licenseKey: "gpl-v3"`) builds a multi-sheet session from the workbook folder.
5. **Overlay-only in 0.4.0:** Live/Build show evaluated values; never materialize computed numbers back into markdown.

## Consequences

- Agents and grep see literals, not totals, unless a later evaluate API exists.
- Cross-sheet refs use HyperFormula sheet names matching `sheets:` stems.
- Orphan `kind: sheet` outside a marked folder can still evaluate alone; full workbook UX prefers the marker.

## Code anchors

- `apps/medousa-home/src/lib/utils/workbook.ts`
- `apps/medousa-home/src/lib/utils/workbookFormulas.ts`
- `apps/medousa-home/src/lib/utils/vaultFrontmatter.ts` (`workbook` / `sheet` kinds)
