import { describe, expect, it } from "vitest";
import {
  findChartFenceIndex,
  resolveLiveChartIndex,
} from "./liveFenceLookup";

const DOC = [
  "# Note",
  "",
  "```chart",
  "title: A",
  "| x | y |",
  "| --- | --- |",
  "| 1 | 2 |",
  "```",
  "",
  "```report",
  "title: Nested",
  "",
  "```chart",
  "title: B",
  "| a | b |",
  "| --- | --- |",
  "| 3 | 4 |",
  "```",
  "```",
  "",
].join("\n");

describe("liveFenceLookup", () => {
  it("finds a standalone chart fence", () => {
    const raw = [
      "```chart",
      "title: A",
      "| x | y |",
      "| --- | --- |",
      "| 1 | 2 |",
      "```",
    ].join("\n");
    expect(findChartFenceIndex(DOC, raw)).toBe(0);
  });

  it("maps nested chart local index through a report host", () => {
    const reportRaw = [
      "```report",
      "title: Nested",
      "",
      "```chart",
      "title: B",
      "| a | b |",
      "| --- | --- |",
      "| 3 | 4 |",
      "```",
      "```",
    ].join("\n");
    expect(resolveLiveChartIndex(DOC, reportRaw, 0)).toBe(1);
  });
});
