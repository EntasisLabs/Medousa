import { describe, expect, it } from "vitest";
import {
  applyMedousaSheetView,
  findLedgerSheetBlock,
  isMedousaSheetConfigEmpty,
  ledgerSheetConfigFromContent,
  parseMedousaSheetBody,
  serializeMedousaSheetFence,
  upsertLedgerSheetFence,
} from "./medousaSheet";

const SAMPLE = `---
kind: ledger
---

\`\`\`medousa-sheet
id: daily
title: Daily tracks
filter: Category != Transfer
sort: -Date
\`\`\`

| Date | Payee | Amount | Category |
| --- | --- | --- | --- |
| 2026-01-02 | Cafe | 12 | Food |
| 2026-01-01 | Bank | 100 | Transfer |
| 2026-01-03 | Shop | 8 | Food |
`;

describe("medousaSheet", () => {
  it("parses sheet body keys", () => {
    const config = parseMedousaSheetBody(`id: daily
title: Tracks
filter: Category != Transfer
filter: Payee = Cafe
sort: -Date`);
    expect(config.id).toBe("daily");
    expect(config.title).toBe("Tracks");
    expect(config.filters).toEqual([
      { column: "Category", op: "!=", value: "Transfer" },
      { column: "Payee", op: "=", value: "Cafe" },
    ]);
    expect(config.sort).toEqual({ column: "Date", descending: true });
  });

  it("finds the sheet fence above the ledger table", () => {
    const block = findLedgerSheetBlock(SAMPLE);
    expect(block?.config.id).toBe("daily");
    expect(ledgerSheetConfigFromContent(SAMPLE).title).toBe("Daily tracks");
  });

  it("serializes and upserts a fence", () => {
    const bare = `---
kind: ledger
---

| Date | Payee | Amount | Category |
| --- | --- | --- | --- |
| a | b | 1 | c |
`;
    const withFence = upsertLedgerSheetFence(bare, {
      filters: [{ column: "Category", op: "=", value: "Food" }],
      sort: { column: "Date", descending: false },
    });
    expect(withFence).toContain("```medousa-sheet");
    expect(withFence).toContain("filter: Category = Food");
    expect(withFence).toContain("sort: Date");

    const cleared = upsertLedgerSheetFence(withFence, { filters: [] });
    expect(cleared).not.toContain("```medousa-sheet");
    expect(isMedousaSheetConfigEmpty({ filters: [] })).toBe(true);
  });

  it("applies filter and sort as a view", () => {
    const columns = [
      { label: "Date", meta: {} },
      { label: "Payee", meta: {} },
      { label: "Amount", meta: {} },
      { label: "Category", meta: {} },
    ];
    const rows = [
      ["2026-01-02", "Cafe", "12", "Food"],
      ["2026-01-01", "Bank", "100", "Transfer"],
      ["2026-01-03", "Shop", "8", "Food"],
    ];
    const view = applyMedousaSheetView(columns, rows, {
      filters: [{ column: "Category", op: "!=", value: "Transfer" }],
      sort: { column: "Date", descending: true },
    });
    expect(view.map((row) => row.sourceIndex)).toEqual([2, 0]);
    expect(view[0].cells[1]).toBe("Shop");
  });

  it("round-trips fence text", () => {
    const fence = serializeMedousaSheetFence({
      id: "main",
      filters: [{ column: "Payee", op: "=", value: "Cafe Latte" }],
      sort: { column: "Amount", descending: true },
    });
    expect(fence).toContain('filter: Payee = "Cafe Latte"');
    expect(fence).toContain("sort: -Amount");
  });
});
