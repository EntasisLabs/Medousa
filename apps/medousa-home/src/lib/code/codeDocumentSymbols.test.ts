import { describe, expect, it } from "vitest";
import {
  containingSymbolTrail,
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
