import { describe, expect, it } from "vitest";
import {
  extractLiquidFences,
  parseAccordionFenceBody,
  parseCalloutFenceBody,
  parseCardFenceBody,
  parseCodeFenceBody,
  parseCompareFenceBody,
  parseDashboardFenceBody,
  parseStepsFenceBody,
  parseTabsFenceBody,
  parseTreeFenceBody,
  replaceLiquidFenceRawAt,
  serializeAccordionFence,
  serializeCalloutFence,
  serializeCardFence,
  serializeCodeFence,
  serializeCompareFence,
  serializeDashboardFence,
  serializeStepsFence,
  serializeTabsFence,
  serializeTreeFence,
} from "./vaultLiquidFence";

describe("vaultLiquidFence", () => {
  it("round-trips a card fence", () => {
    const draft = {
      title: "Summary",
      subtitle: "Context",
      emoji: "📋",
      body: "Hello",
      meta: "v1",
      points: [{ label: "A", value: "1" }],
    };
    const raw = serializeCardFence(draft);
    const body = extractLiquidFences(raw, "card")[0]?.body ?? "";
    expect(parseCardFenceBody(body)).toEqual(draft);
  });

  it("round-trips a callout fence", () => {
    const draft = { tone: "warn", title: "Aside", body: "Careful" };
    const raw = serializeCalloutFence(draft);
    const body = extractLiquidFences(raw, "callout")[0]?.body ?? "";
    expect(parseCalloutFenceBody(body)).toEqual(draft);
  });

  it("round-trips dashboard tiles", () => {
    const draft = {
      title: "At a glance",
      columns: "2",
      tiles: [
        { label: "Metric", value: "42", tone: "success", delta: "+1" },
        { label: "Status", value: "On track", tone: "accent", delta: "" },
      ],
    };
    const raw = serializeDashboardFence(draft);
    const body = extractLiquidFences(raw, "dashboard")[0]?.body ?? "";
    expect(parseDashboardFenceBody(body)).toEqual(draft);
  });

  it("round-trips tabs panels", () => {
    const draft = {
      title: "Getting started",
      defaultLabel: "Run",
      panels: [
        { label: "Install", body: "npm i" },
        { label: "Run", body: "npm start" },
      ],
    };
    const raw = serializeTabsFence(draft);
    const body = extractLiquidFences(raw, "tabs")[0]?.body ?? "";
    expect(parseTabsFenceBody(body)).toEqual(draft);
  });

  it("round-trips steps", () => {
    const draft = {
      title: "Ship it",
      steps: [
        { label: "Build", body: "cargo build", status: "done" },
        { label: "Test", body: "cargo test", status: "current" },
      ],
    };
    const raw = serializeStepsFence(draft);
    const body = extractLiquidFences(raw, "steps")[0]?.body ?? "";
    expect(parseStepsFenceBody(body)).toEqual(draft);
  });

  it("round-trips accordion items", () => {
    const draft = {
      title: "FAQ",
      multiple: false,
      items: [
        { label: "What?", body: "Liquid", open: true },
        { label: "Why?", body: "Agents", open: false },
      ],
    };
    const raw = serializeAccordionFence(draft);
    const body = extractLiquidFences(raw, "accordion")[0]?.body ?? "";
    expect(parseAccordionFenceBody(body)).toEqual(draft);
  });

  it("round-trips code source", () => {
    const draft = {
      lang: "typescript",
      title: "greet.ts",
      source: "export const x = 1;",
    };
    const raw = serializeCodeFence(draft);
    const body = extractLiquidFences(raw, "code")[0]?.body ?? "";
    expect(parseCodeFenceBody(body)).toEqual(draft);
  });

  it("round-trips tree text", () => {
    const draft = {
      title: "Project",
      treeText: "src/\n  index.ts",
    };
    const raw = serializeTreeFence(draft);
    const body = extractLiquidFences(raw, "tree")[0]?.body ?? "";
    expect(parseTreeFenceBody(body)).toEqual(draft);
  });

  it("round-trips compare table + faceoff mode", () => {
    const draft = {
      title: "Head to head",
      subtitle: "Live polish",
      recommendation: "Alpha",
      mode: "faceoff" as const,
      tableMarkdown: [
        "| | Alpha | Beta |",
        "| --- | --- | --- |",
        "| Speed | Fast | Slow |",
        "| Taste | High | Medium |",
      ].join("\n"),
    };
    const raw = serializeCompareFence(draft);
    expect(raw).toContain("mode: faceoff");
    const body = extractLiquidFences(raw, "compare")[0]?.body ?? "";
    const parsed = parseCompareFenceBody(body);
    expect(parsed.title).toBe("Head to head");
    expect(parsed.subtitle).toBe("Live polish");
    expect(parsed.recommendation).toBe("Alpha");
    expect(parsed.mode).toBe("faceoff");
    expect(parsed.tableMarkdown).toContain("Alpha");
    expect(parsed.tableMarkdown).toContain("Speed");
  });

  it("omits matrix mode from compare serialize (default)", () => {
    const raw = serializeCompareFence({
      title: "Grid",
      subtitle: "",
      recommendation: "",
      mode: "matrix",
      tableMarkdown: [
        "| | A | B |",
        "| --- | --- | --- |",
        "| X | 1 | 2 |",
      ].join("\n"),
    });
    expect(raw).not.toContain("mode:");
  });

  it("replaces the Nth card fence", () => {
    const source = [
      "```card",
      "title: One",
      "body: a",
      "```",
      "",
      "```card",
      "title: Two",
      "body: b",
      "```",
      "",
    ].join("\n");
    const next = replaceLiquidFenceRawAt(
      source,
      "card",
      1,
      serializeCardFence({
        title: "Two*",
        subtitle: "",
        emoji: "",
        body: "b*",
        meta: "",
        points: [],
      }),
    );
    expect(next).toContain("title: Two*");
    expect(next).toContain("title: One");
    expect(extractLiquidFences(next!, "card")).toHaveLength(2);
  });
});
