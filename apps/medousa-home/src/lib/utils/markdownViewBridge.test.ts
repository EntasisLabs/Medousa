import { describe, expect, it } from "vitest";
import {
  parseViewBlockBody,
  replaceMedousaViewFenceAt,
  serializeMedousaViewFence,
  serializeMedousaViewQuery,
} from "./markdownView";
import {
  serializeCalloutBlock,
  serializeTransclusion,
  replaceSlashWith,
} from "./vaultMarkdownEdit";

describe("serializeMedousaViewQuery", () => {
  it("round-trips a full query", () => {
    const body = serializeMedousaViewQuery({
      from: "projects/data.md",
      table: "first",
      wheres: [{ column: "status", op: "!=", value: "done" }],
      sort: { column: "due", descending: false },
      columns: ["name", "status", "due"],
    });
    const parsed = parseViewBlockBody(body);
    expect(parsed).toEqual({
      from: "projects/data.md",
      table: "first",
      wheres: [{ column: "status", op: "!=", value: "done" }],
      sort: { column: "due", descending: false },
      columns: ["name", "status", "due"],
    });
  });

  it("quotes filter values with spaces", () => {
    const body = serializeMedousaViewQuery({
      from: "finance/tracks.md",
      table: "ledger",
      wheres: [{ column: "Payee", op: "=", value: "Blue Bottle" }],
    });
    expect(body).toContain('where: Payee = "Blue Bottle"');
    expect(parseViewBlockBody(body)?.wheres[0]?.value).toBe("Blue Bottle");
  });

  it("wraps a fence for insertion", () => {
    const fence = serializeMedousaViewFence({
      from: "projects/data.md",
      table: "first",
      wheres: [],
    });
    expect(fence.startsWith("```medousa-view\n")).toBe(true);
    expect(fence).toContain("from: projects/data.md");
    expect(fence.endsWith("```\n\n")).toBe(true);
  });
});

describe("replaceMedousaViewFenceAt", () => {
  it("updates the targeted fence only", () => {
    const source = [
      "Intro",
      "",
      "```medousa-view",
      "from: a.md",
      "table: first",
      "```",
      "",
      "```medousa-view",
      "from: b.md",
      "table: ledger",
      "```",
      "",
    ].join("\n");
    const next = replaceMedousaViewFenceAt(source, 1, {
      from: "finance/tracks.md",
      table: "ledger",
      wheres: [{ column: "Category", op: "!=", value: "Transfer" }],
    });
    expect(next).toContain("from: a.md");
    expect(next).toContain("from: finance/tracks.md");
    expect(next).not.toContain("from: b.md");
    expect(next).toContain("where: Category != Transfer");
  });
});

describe("callout and embed serializers", () => {
  it("builds an Obsidian callout", () => {
    expect(serializeCalloutBlock("warning", "Heads up", "Check the path")).toBe(
      "> [!warning] Heads up\n> Check the path\n\n",
    );
  });

  it("builds a transclusion token", () => {
    expect(serializeTransclusion("journal/2026-07-11.md")).toBe(
      "![[journal/2026-07-11]]\n\n",
    );
  });
});

describe("replaceSlashWith", () => {
  it("clears a slash token", () => {
    const result = replaceSlashWith("hello\n/view", 12, "");
    expect(result.content).toBe("hello\n");
    expect(result.selectionStart).toBe(6);
  });

  it("inserts a bridge payload at the slash", () => {
    const result = replaceSlashWith("\n/call", 6, "> [!note] note\n> \n\n");
    expect(result.content).toContain("> [!note] note");
    expect(result.content.includes("/call")).toBe(false);
  });
});
