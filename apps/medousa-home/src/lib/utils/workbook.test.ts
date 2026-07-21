import { describe, expect, it } from "vitest";
import {
  a1ToRowCol,
  createEmptyWorkbookMarker,
  isValidA1,
  normalizeA1,
  parseSheetFormulas,
  parseWorkbookManifest,
  rowColToA1,
  serializeWorkbookManifest,
  setSheetFormulasYaml,
  upsertSheetFormulas,
} from "./workbook";

describe("workbook A1", () => {
  it("normalizes and validates addresses", () => {
    expect(normalizeA1("b12")).toBe("B12");
    expect(normalizeA1("$C$3")).toBe("C3");
    expect(isValidA1("AA1")).toBe(true);
    expect(isValidA1("A0")).toBe(false);
    expect(a1ToRowCol("B3")).toEqual({ row: 2, col: 1 });
    expect(rowColToA1(0, 0)).toBe("A1");
  });
});

describe("workbook manifest", () => {
  it("parses _workbook.md frontmatter", () => {
    const md = `---
kind: workbook
title: Q3 budget
sheets:
  - Revenue
  - Costs
  - Summary
---
`;
    const m = parseWorkbookManifest(md);
    expect(m).toEqual({
      title: "Q3 budget",
      sheets: ["Revenue", "Costs", "Summary"],
    });
  });

  it("round-trips serialize → parse", () => {
    const md = serializeWorkbookManifest({
      title: "Demo",
      sheets: ["A", "B.md"],
    });
    expect(parseWorkbookManifest(md)).toEqual({
      title: "Demo",
      sheets: ["A", "B"],
    });
    expect(createEmptyWorkbookMarker("X", ["Sheet1"])).toContain("kind: workbook");
  });
});

describe("sheet formulas frontmatter", () => {
  it("parses formulas map", () => {
    const md = `---
kind: sheet
sheet: Revenue
formulas:
  B12: "=SUM(B2:B11)"
  C3: "=Costs!B10*1.1"
---

| Item | Amount |
| ---- | ------ |
| A    | 10     |
`;
    const { formulas } = parseSheetFormulas(md);
    expect(formulas.B12).toBe("=SUM(B2:B11)");
    expect(formulas.C3).toBe("=Costs!B10*1.1");
  });

  it("upserts formulas without touching table body", () => {
    const md = `---
kind: sheet
---

| A | B |
| - | - |
| 1 | 2 |
`;
    const next = upsertSheetFormulas(md, { B2: "=A2*2" });
    expect(next).toContain('B2: "=A2*2"');
    expect(next).toContain("| 1 | 2 |");
    const fm = setSheetFormulasYaml("kind: sheet", { A1: "=1+1" });
    expect(fm).toContain("formulas:");
    expect(fm).toContain("A1:");
  });
});
