import { describe, expect, it } from "vitest";

import { markdownTableShellClass, renderMarkdown } from "./render";

describe("markdown table matrix shell", () => {
  it("marks short 3-col tables as matrix", () => {
    const table = `<table><thead><tr><th>A</th><th>B</th><th>C</th></tr></thead><tbody><tr><td>One</td><td>Two</td><td>Three</td></tr></tbody></table>`;
    expect(markdownTableShellClass(table)).toBe(
      "markdown-table-scroll markdown-table--matrix",
    );
  });

  it("keeps narrative tables as plain scroll shells", () => {
    const long =
      "A long narrative cell that describes an entire project outcome with enough text to exceed the matrix heuristic.";
    const table = `<table><tr><td>${long}</td><td>${long}</td><td>${long}</td></tr></table>`;
    expect(markdownTableShellClass(table)).toBe("markdown-table-scroll");
  });

  it("applies matrix class through renderMarkdown", () => {
    const html = renderMarkdown(
      [
        "| Logistics | Vendor | Reporting |",
        "| --- | --- | --- |",
        "| Dispatch | Carriers | Dashboards |",
      ].join("\n"),
    );
    expect(html).toContain("markdown-table--matrix");
  });
});
