import { describe, expect, it } from "vitest";
import {
  findLedgerTable,
  isLedgerHeaders,
  ledgerProtectedColumnIndexes,
  replaceLedgerTable,
  serializePipeTable,
} from "./markdownTable";

describe("markdownTable ledger", () => {
  it("detects classic ledger headers", () => {
    expect(isLedgerHeaders(["Date", "Payee", "Amount", "Category"])).toBe(true);
    expect(isLedgerHeaders(["Date", "Payee", "Amount"])).toBe(false);
  });

  it("detects enriched core headers", () => {
    expect(
      isLedgerHeaders([
        "{{Date type:date}}",
        "Payee",
        "{{Amount type:currency}}",
        "Category",
      ]),
    ).toBe(true);
  });

  it("finds extended tables on ledger notes", () => {
    const markdown = `---
kind: ledger
---

| Date | Payee | Amount | Category | Notes |
| --- | --- | --- | --- | --- |
| 2026-01-01 | Cafe | 12 | Food | latte |
`;
    const table = findLedgerTable(markdown);
    expect(table?.headers).toEqual([
      "Date",
      "Payee",
      "Amount",
      "Category",
      "Notes",
    ]);
    expect(table?.rows).toHaveLength(1);
  });

  it("replaces headers and rows together", () => {
    const markdown = `---
kind: ledger
---

| Date | Payee | Amount | Category |
| --- | --- | --- | --- |
| a | b | 1 | c |
`;
    const next = replaceLedgerTable(
      markdown,
      [
        ["a", "b", "1", "c", "x"],
        ["", "", "", "", ""],
      ],
      ["Date", "Payee", "Amount", "Category", "Notes"],
    );
    expect(next).toContain("| Date | Payee | Amount | Category | Notes |");
    expect(next).toContain("| a | b | 1 | c | x |");
  });

  it("protects core columns by display label", () => {
    const indexes = ledgerProtectedColumnIndexes([
      "{{Date type:date}}",
      "Payee",
      "{{Amount type:currency}}",
      "Category",
      "Notes",
    ]);
    expect([...indexes].sort()).toEqual([0, 1, 2, 3]);
  });

  it("serializes pipe tables", () => {
    expect(serializePipeTable(["A", "B"], [["1", "2"]])).toBe(
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
    );
  });
});
