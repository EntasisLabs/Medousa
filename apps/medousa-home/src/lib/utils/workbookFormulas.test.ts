import { describe, expect, it } from "vitest";
import { evaluateWorkbookOverlay, evaluateSheetOverlay } from "./workbookFormulas";

describe("workbookFormulas overlay", () => {
  it("evaluates SUM without writing computed cells into markdown", () => {
    const manifest = `---
kind: workbook
title: Demo
sheets:
  - Revenue
---
`;
    const revenue = `---
kind: sheet
formulas:
  B3: "=SUM(B2:B2)"
---

| Item | Amount |
| ---- | ------ |
| Widget | 40 |
`;
    const overlay = evaluateWorkbookOverlay(manifest, [
      { name: "Revenue", markdown: revenue },
    ]);
    expect(overlay?.title).toBe("Demo");
    expect(overlay?.sheets[0]?.cells.B3?.display).toBe("40");
    expect(revenue).not.toContain("| 40 | 40 |");
  });

  it("resolves cross-sheet references", () => {
    const manifest = `---
kind: workbook
title: Cross
sheets:
  - Costs
  - Summary
---
`;
    const costs = `---
kind: sheet
---

| Label | Value |
| ----- | ----- |
| Rent  | 100   |
`;
    const summary = `---
kind: sheet
formulas:
  B2: "=Costs!B2*1.1"
---

| Label | Value |
| ----- | ----- |
| Total |       |
`;
    const overlay = evaluateWorkbookOverlay(manifest, [
      { name: "Costs", markdown: costs },
      { name: "Summary", markdown: summary },
    ]);
    expect(overlay?.sheets.find((s) => s.name === "Summary")?.cells.B2?.display).toBe(
      "110",
    );
  });

  it("evaluates a lone sheet", () => {
    const md = `---
kind: sheet
formulas:
  A2: "=1+2"
---

| N |
| - |
|   |
`;
    const overlay = evaluateSheetOverlay("Solo", md);
    expect(overlay.cells.A2?.display).toBe("3");
  });
});
