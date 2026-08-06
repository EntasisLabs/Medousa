import { describe, expect, it } from "vitest";
import {
  countCodeProblems,
  filterCodeProblems,
  groupCodeProblems,
  normalizeCodeWorkspaceProblems,
} from "./codeProblems";

const root = "/work/project";

describe("workspace Problems", () => {
  const documents = [
    {
      uri: "file:///work/project/src/b.ts",
      language: "typescript",
      diagnostics: [
        {
          message: "Unused value",
          severity: 2,
          source: "ts",
          code: 6133,
          tags: [1],
          range: {
            start: { line: 4, character: 2 },
            end: { line: 4, character: 7 },
          },
          relatedInformation: [{
            location: {
              uri: "file:///work/project/src/a.ts",
              range: { start: { line: 1, character: 3 } },
            },
            message: "Declared here",
          }],
        },
      ],
    },
    {
      uri: "file:///work/project/src/a.ts",
      language: "typescript",
      diagnostics: [
        {
          message: "Type mismatch",
          severity: 1,
          source: "ts",
          code: { value: "E100" },
          range: { start: { line: 1, character: 3 }, end: { line: 1, character: 4 } },
        },
        {
          message: "Consider a more specific type",
          severity: 4,
          range: { start: { line: 8, character: 0 }, end: { line: 8, character: 1 } },
        },
      ],
    },
  ];

  it("normalizes, deduplicates, and sorts complete LSP diagnostics", () => {
    const problems = normalizeCodeWorkspaceProblems(
      [...documents, documents[0]],
      root,
    );
    expect(problems).toHaveLength(3);
    expect(problems.map((problem) => [problem.path, problem.severity, problem.line])).toEqual([
      ["src/a.ts", "error", 2],
      ["src/b.ts", "warning", 5],
      ["src/a.ts", "hint", 9],
    ]);
    expect(problems[0]).toMatchObject({ character: 4, code: "E100", source: "ts" });
    expect(problems[1].relatedInformation).toEqual([
      {
        uri: "file:///work/project/src/a.ts",
        path: "src/a.ts",
        line: 2,
        character: 4,
        message: "Declared here",
      },
    ]);
  });

  it("rejects diagnostics outside the governed project root", () => {
    expect(
      normalizeCodeWorkspaceProblems(
        [{ uri: "file:///another/project/a.ts", diagnostics: [{ message: "No" }] }],
        root,
      ),
    ).toEqual([]);
  });

  it("filters by severity and project-wide text", () => {
    const problems = normalizeCodeWorkspaceProblems(documents, root);
    expect(filterCodeProblems(problems, { severity: "warning" })).toHaveLength(1);
    expect(filterCodeProblems(problems, { severity: "information" })).toHaveLength(1);
    expect(filterCodeProblems(problems, { query: "E100" })).toHaveLength(1);
    expect(filterCodeProblems(problems, { query: "src/b" })[0]?.message).toBe("Unused value");
  });

  it("groups by file with accurate severity counts", () => {
    const problems = normalizeCodeWorkspaceProblems(documents, root);
    expect(countCodeProblems(problems)).toEqual({
      total: 3,
      errors: 1,
      warnings: 1,
      information: 0,
      hints: 1,
    });
    expect(groupCodeProblems(problems).map((group) => ({
      path: group.path,
      total: group.counts.total,
      errors: group.counts.errors,
    }))).toEqual([
      { path: "src/a.ts", total: 2, errors: 1 },
      { path: "src/b.ts", total: 1, errors: 0 },
    ]);
  });
});
