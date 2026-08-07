import { describe, expect, it } from "vitest";
import {
  containingSymbolTrail,
  collapsePathBreadcrumbs,
  collapseSymbolTrail,
  pathBreadcrumbSegments,
} from "./codeDocumentSymbols";
import type { CodeDocumentSymbol } from "./codingEngineClient";

describe("pathBreadcrumbSegments", () => {
  it("splits nested paths into clickable segments", () => {
    expect(pathBreadcrumbSegments("src/lib/foo.ts")).toEqual([
      { label: "src", path: "src", isFile: false },
      { label: "lib", path: "src/lib", isFile: false },
      { label: "foo.ts", path: "src/lib/foo.ts", isFile: true },
    ]);
  });
});

describe("collapsePathBreadcrumbs", () => {
  it("keeps short paths intact", () => {
    const segments = pathBreadcrumbSegments("src/foo.ts");
    expect(collapsePathBreadcrumbs(segments)).toEqual(segments);
  });

  it("collapses the middle of deep paths", () => {
    const segments = pathBreadcrumbSegments(
      "crates/medousa-install-support/src/catalog/model_catalog.rs",
    );
    expect(collapsePathBreadcrumbs(segments)).toEqual([
      { label: "crates", path: "crates", isFile: false },
      { label: "…", path: "", isFile: false, ellipsis: true },
      { label: "catalog", path: "crates/medousa-install-support/src/catalog", isFile: false },
      {
        label: "model_catalog.rs",
        path: "crates/medousa-install-support/src/catalog/model_catalog.rs",
        isFile: true,
      },
    ]);
  });
});

describe("collapseSymbolTrail", () => {
  it("keeps only the leaf by default", () => {
    expect(
      collapseSymbolTrail([
        { name: "Outer", line: 1 },
        { name: "inner", line: 11 },
      ]),
    ).toEqual([{ name: "inner", line: 11 }]);
  });
});

describe("containingSymbolTrail", () => {
  const nested: CodeDocumentSymbol[] = [
    {
      name: "Outer",
      range: { start: { line: 0 }, end: { line: 40 } },
      selectionRange: { start: { line: 0 } },
      children: [
        {
          name: "inner",
          range: { start: { line: 10 }, end: { line: 20 } },
          selectionRange: { start: { line: 10 } },
        },
      ],
    },
  ];

  it("returns outermost to innermost containing symbols", () => {
    expect(containingSymbolTrail(nested, 15)).toEqual([
      { name: "Outer", line: 1 },
      { name: "inner", line: 11 },
    ]);
  });

  it("falls back for flat symbols without end ranges", () => {
    const flat: CodeDocumentSymbol[] = [
      { name: "a", selectionRange: { start: { line: 0 } } },
      { name: "b", selectionRange: { start: { line: 8 } } },
    ];
    expect(containingSymbolTrail(flat, 10)).toEqual([{ name: "b", line: 9 }]);
  });
});
