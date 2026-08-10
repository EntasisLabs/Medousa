import { describe, expect, it } from "vitest";
import type { ForgeSourceFile } from "$lib/forge";
import { pathToFileUri } from "./codeDocumentUri";
import {
  applyCodeTextEdits,
  buildCodeWorkspaceEditPlan,
  CodeWorkspaceEditError,
} from "./codeWorkspaceEdit";

const root = "/work/project";
const uri = (path: string) => pathToFileUri(`${root}/${path}`);

function source(path: string, content: string, digest = `digest-${path}`): ForgeSourceFile {
  return {
    work_id: "work-1",
    path,
    content,
    digest,
    byte_size: new TextEncoder().encode(content).byteLength,
  };
}

function loader(entries: Record<string, ForgeSourceFile | null>) {
  return async (path: string) => entries[path] ?? null;
}

describe("code workspace edit planning", () => {
  it("normalizes deterministic multi-file text changes with digest preconditions", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        changes: {
          [uri("src/b.ts")]: [
            {
              range: { start: { line: 0, character: 6 }, end: { line: 0, character: 7 } },
              newText: "2",
            },
          ],
          [uri("src/a.ts")]: [
            {
              range: { start: { line: 0, character: 6 }, end: { line: 0, character: 7 } },
              newText: "1",
            },
          ],
        },
      },
      {
        workspaceRoot: root,
        loadSource: loader({
          "src/a.ts": source("src/a.ts", "const a=0"),
          "src/b.ts": source("src/b.ts", "const b=0"),
        }),
      },
    );

    expect(plan.operations).toEqual([
      { kind: "write", path: "src/a.ts", content: "const 1=0" },
      { kind: "write", path: "src/b.ts", content: "const 2=0" },
    ]);
    expect(plan.preconditions).toEqual([
      { kind: "existing", path: "src/a.ts", expected_digest: "digest-src/a.ts" },
      { kind: "existing", path: "src/b.ts", expected_digest: "digest-src/b.ts" },
    ]);
    expect(plan.files.map(({ path, status, before, after }) => ({ path, status, before, after }))).toEqual([
      { path: "src/a.ts", status: "modified", before: "const a=0", after: "const 1=0" },
      { path: "src/b.ts", status: "modified", before: "const b=0", after: "const 2=0" },
    ]);
  });

  it("preserves create, edit, and rename ordering for a previously absent file", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        documentChanges: [
          { kind: "create", uri: uri("src/new.ts") },
          {
            textDocument: { uri: uri("src/new.ts"), version: null },
            edits: [
              {
                range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
                newText: "export const answer = 42;\n",
              },
            ],
          },
          { kind: "rename", oldUri: uri("src/new.ts"), newUri: uri("src/final.ts") },
        ],
      },
      { workspaceRoot: root, loadSource: loader({}) },
    );

    expect(plan.operations).toEqual([
      { kind: "create", path: "src/new.ts", content: "" },
      { kind: "write", path: "src/new.ts", content: "export const answer = 42;\n" },
      { kind: "rename", path: "src/new.ts", destination: "src/final.ts" },
    ]);
    expect(plan.preconditions).toEqual([
      { kind: "missing", path: "src/new.ts" },
      { kind: "missing", path: "src/final.ts" },
    ]);
    expect(plan.files).toEqual([
      {
        id: "created:src/final.ts",
        path: "src/final.ts",
        status: "created",
        before: "",
        after: "export const answer = 42;\n",
      },
    ]);
  });

  it("shows both the replaced destination and the source identity for overwrite rename", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        documentChanges: [
          {
            kind: "rename",
            oldUri: uri("src/old.ts"),
            newUri: uri("src/existing.ts"),
            options: { overwrite: true, ignoreIfExists: true },
          },
        ],
      },
      {
        workspaceRoot: root,
        loadSource: loader({
          "src/old.ts": source("src/old.ts", "source"),
          "src/existing.ts": source("src/existing.ts", "destination"),
        }),
      },
    );

    expect(plan.operations).toEqual([
      { kind: "delete", path: "src/existing.ts" },
      { kind: "rename", path: "src/old.ts", destination: "src/existing.ts" },
    ]);
    expect(plan.files).toEqual([
      {
        id: "renamed:src/old.ts:src/existing.ts",
        path: "src/existing.ts",
        oldPath: "src/old.ts",
        status: "renamed",
        before: "source",
        after: "source",
      },
      {
        id: "deleted:src/existing.ts",
        path: "src/existing.ts",
        status: "deleted",
        before: "destination",
        after: "",
      },
    ]);
  });

  it("honors ignore options without inventing transaction work", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        documentChanges: [
          { kind: "create", uri: uri("src/a.ts"), options: { ignoreIfExists: true } },
          {
            kind: "rename",
            oldUri: uri("src/a.ts"),
            newUri: uri("src/b.ts"),
            options: { ignoreIfExists: true },
          },
          { kind: "delete", uri: uri("src/missing.ts"), options: { ignoreIfNotExists: true } },
        ],
      },
      {
        workspaceRoot: root,
        loadSource: loader({
          "src/a.ts": source("src/a.ts", "a"),
          "src/b.ts": source("src/b.ts", "b"),
        }),
      },
    );

    expect(plan.operations).toEqual([]);
    expect(plan.preconditions).toEqual([]);
    expect(plan.files).toEqual([]);
  });

  it("uses write semantics when create-overwrite truncates an existing file", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        documentChanges: [
          { kind: "create", uri: uri("src/a.ts"), options: { overwrite: true } },
        ],
      },
      {
        workspaceRoot: root,
        loadSource: loader({ "src/a.ts": source("src/a.ts", "old") }),
      },
    );

    expect(plan.operations).toEqual([{ kind: "write", path: "src/a.ts", content: "" }]);
    expect(plan.files[0]).toMatchObject({
      path: "src/a.ts",
      status: "modified",
      before: "old",
      after: "",
    });
  });

  it("rejects URIs outside the authoritative workshop root", async () => {
    await expect(
      buildCodeWorkspaceEditPlan(
        {
          changes: {
            "file:///another/project/a.ts": [],
          },
        },
        { workspaceRoot: root, loadSource: loader({}) },
      ),
    ).rejects.toThrow(/outside this project/);
  });

  it("rejects missing and invalid source snapshots before producing operations", async () => {
    await expect(
      buildCodeWorkspaceEditPlan(
        {
          changes: {
            [uri("src/missing.ts")]: [
              {
                range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
                newText: "x",
              },
            ],
          },
        },
        { workspaceRoot: root, loadSource: loader({}) },
      ),
    ).rejects.toThrow(/does not exist/);

    await expect(
      buildCodeWorkspaceEditPlan(
        { changes: { [uri("src/a.ts")]: [] } },
        {
          workspaceRoot: root,
          loadSource: async () => source("src/not-a.ts", ""),
        },
      ),
    ).rejects.toThrow(/invalid source snapshot/);
  });

  it("collects only annotations referenced by the edit", async () => {
    const plan = await buildCodeWorkspaceEditPlan(
      {
        changeAnnotations: {
          rename: { label: "Rename public symbol", needsConfirmation: true },
          unused: { label: "Unused label" },
        },
        documentChanges: [
          {
            textDocument: { uri: uri("src/a.ts") },
            edits: [
              {
                range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
                newText: "b",
                annotationId: "rename",
              },
            ],
          },
        ],
      },
      {
        workspaceRoot: root,
        loadSource: loader({ "src/a.ts": source("src/a.ts", "a") }),
      },
    );

    expect(plan.annotationLabels).toEqual(["Rename public symbol"]);
  });
});

describe("LSP text edit application", () => {
  it("uses UTF-16 positions and preserves CRLF line endings", () => {
    expect(
      applyCodeTextEdits("a😀b\r\nz", [
        {
          range: { start: { line: 0, character: 1 }, end: { line: 0, character: 3 } },
          newText: "X",
        },
        {
          range: { start: { line: 1, character: 1 }, end: { line: 1, character: 1 } },
          newText: "!",
        },
      ]),
    ).toBe("aXb\r\nz!");
  });

  it("preserves array order for insertions at the same position", () => {
    expect(
      applyCodeTextEdits("x", [
        {
          range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
          newText: "a",
        },
        {
          range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
          newText: "b",
        },
      ]),
    ).toBe("abx");
  });

  it("rejects overlapping, reversed, and out-of-range edits", () => {
    expect(() =>
      applyCodeTextEdits("abcd", [
        {
          range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } },
          newText: "x",
        },
        {
          range: { start: { line: 0, character: 2 }, end: { line: 0, character: 4 } },
          newText: "y",
        },
      ]),
    ).toThrow(CodeWorkspaceEditError);
    expect(() =>
      applyCodeTextEdits("abcd", [
        {
          range: { start: { line: 0, character: 3 }, end: { line: 0, character: 2 } },
          newText: "x",
        },
      ]),
    ).toThrow(/reversed/);
    expect(() =>
      applyCodeTextEdits("abcd", [
        {
          range: { start: { line: 1, character: 0 }, end: { line: 1, character: 0 } },
          newText: "x",
        },
      ]),
    ).toThrow(/outside/);
  });
});
