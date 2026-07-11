import { describe, expect, it } from "vitest";
import {
  columnDisplayLabel,
  mergeColumnMeta,
  parseColumnHeader,
  resolveColumnAlign,
  serializeColumnHeader,
  serializeLedgerColumns,
} from "./ledgerSheet";

describe("ledgerSheet column enrichment", () => {
  it("parses plain headers", () => {
    expect(parseColumnHeader("Date")).toEqual({ label: "Date", meta: {} });
    expect(parseColumnHeader("  Payee  ")).toEqual({ label: "Payee", meta: {} });
  });

  it("parses enriched headers without pipes", () => {
    expect(
      parseColumnHeader("{{Date width:8rem type:date}}"),
    ).toEqual({
      label: "Date",
      meta: { width: "8rem", type: "date" },
    });
    expect(
      parseColumnHeader("{{Amount type:currency align:right color:green}}"),
    ).toEqual({
      label: "Amount",
      meta: { type: "currency", align: "right", color: "green" },
    });
  });

  it("supports multi-word labels before meta", () => {
    expect(parseColumnHeader("{{Due Date type:date width:7rem}}")).toEqual({
      label: "Due Date",
      meta: { type: "date", width: "7rem" },
    });
  });

  it("ignores invalid meta values", () => {
    expect(parseColumnHeader("{{Notes width:huge color:neon}}")).toEqual({
      label: "Notes",
      meta: {},
    });
  });

  it("round-trips serialize → parse", () => {
    const raw = serializeColumnHeader({
      label: "Amount",
      meta: { type: "currency", align: "right", width: "6rem", color: "green" },
    });
    expect(raw).toBe("{{Amount width:6rem type:currency align:right color:green}}");
    expect(parseColumnHeader(raw)).toEqual({
      label: "Amount",
      meta: { type: "currency", align: "right", width: "6rem", color: "green" },
    });
  });

  it("omits braces when meta is empty", () => {
    expect(serializeColumnHeader({ label: "Payee", meta: {} })).toBe("Payee");
  });

  it("serializes a column list for the table header row", () => {
    expect(
      serializeLedgerColumns([
        { label: "Date", meta: { type: "date" } },
        { label: "Payee", meta: {} },
      ]),
    ).toEqual(["{{Date type:date}}", "Payee"]);
  });

  it("exposes display labels for protected-column checks", () => {
    expect(columnDisplayLabel("{{Date width:8rem}}")).toBe("Date");
  });

  it("merges and clears meta patches", () => {
    const base = parseColumnHeader("{{Notes width:10rem color:blue}}");
    expect(mergeColumnMeta(base, { width: "12rem", color: undefined })).toEqual({
      label: "Notes",
      meta: { width: "12rem" },
    });
  });

  it("resolves align from type and heuristics", () => {
    expect(
      resolveColumnAlign({ label: "Amount", meta: { type: "currency" } }, 9),
    ).toBe("right");
    expect(resolveColumnAlign({ label: "Payee", meta: {} }, 1)).toBe("left");
    expect(
      resolveColumnAlign({ label: "Payee", meta: { align: "center" } }, 1),
    ).toBe("center");
  });
});
