import { ChangeSet, EditorState, Text, type TransactionSpec } from "@codemirror/state";
import type { LSPClient, WorkspaceFile } from "@codemirror/lsp-client";
import type { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import {
  MedousaCodeWorkspace,
  MedousaCodeWorkspaceBridge,
  MedousaCodeWorkspaceConflictError,
  canonicalCodeDocumentUri,
} from "./medousaCodeWorkspace";

type TestPlugin = {
  syncedDoc: Text;
  unsyncedChanges: ChangeSet;
  clear: () => void;
};

class TestView {
  state: EditorState;
  hasFocus = false;
  readonly lsp: TestPlugin;

  constructor(doc: string) {
    this.state = EditorState.create({ doc });
    this.lsp = {
      syncedDoc: this.state.doc,
      unsyncedChanges: ChangeSet.empty(this.state.doc.length),
      clear: () => {
        this.lsp.syncedDoc = this.state.doc;
        this.lsp.unsyncedChanges = ChangeSet.empty(this.state.doc.length);
      },
    };
  }

  plugin() {
    return this.lsp;
  }

  dispatch(update: TransactionSpec) {
    const transaction = this.state.update(update);
    if (transaction.docChanged) {
      this.lsp.unsyncedChanges = this.lsp.unsyncedChanges.compose(
        transaction.changes,
      );
    }
    this.state = transaction.state;
  }
}

function asView(view: TestView): EditorView {
  return view as unknown as EditorView;
}

function createWorkspace() {
  const opened: WorkspaceFile[] = [];
  const closed: string[] = [];
  const client = {
    didOpen(file: WorkspaceFile) {
      opened.push(file);
    },
    didClose(uri: string) {
      closed.push(uri);
    },
  } as unknown as LSPClient;
  const bridge = new MedousaCodeWorkspaceBridge();
  return {
    bridge,
    workspace: new MedousaCodeWorkspace(client, bridge),
    opened,
    closed,
  };
}

describe("MedousaCodeWorkspace", () => {
  it("opens one LSP document for multiple views and closes the last view", () => {
    const { workspace, opened, closed } = createWorkspace();
    const first = new TestView("const value = 1;\n");
    const second = new TestView("const value = 1;\n");

    workspace.openFile("file:///repo/a.ts", "typescript", asView(first));
    workspace.openFile("file:///repo/a.ts", "typescript", asView(second));

    expect(workspace.files).toHaveLength(1);
    expect(opened).toHaveLength(1);
    expect(workspace.files[0].getView(asView(first))).toBe(asView(first));

    workspace.closeFile("file:///repo/a.ts", asView(first));
    expect(closed).toEqual([]);
    workspace.closeFile("file:///repo/a.ts", asView(second));
    expect(closed).toEqual(["file:///repo/a.ts"]);
    expect(workspace.files).toEqual([]);
  });

  it("synchronizes one edit and mirrors it into an unchanged peer view", () => {
    const { workspace } = createWorkspace();
    const first = new TestView("one\n");
    const second = new TestView("one\n");
    workspace.openFile("file:///repo/a.ts", "typescript", asView(first));
    workspace.openFile("file:///repo/a.ts", "typescript", asView(second));

    first.dispatch({ changes: { from: 3, insert: " two" } });
    const updates = workspace.syncFiles();

    expect(updates).toHaveLength(1);
    expect(updates[0].prevDoc.toString()).toBe("one\n");
    expect(updates[0].file.doc.toString()).toBe("one two\n");
    expect(updates[0].file.version).toBe(1);
    expect(second.state.doc.toString()).toBe("one two\n");
    expect(first.lsp.unsyncedChanges.empty).toBe(true);
    expect(second.lsp.unsyncedChanges.empty).toBe(true);
  });

  it("refuses to collapse divergent split drafts", () => {
    const { workspace } = createWorkspace();
    const first = new TestView("one\n");
    const second = new TestView("one\n");
    workspace.openFile("file:///repo/a.ts", "typescript", asView(first));
    workspace.openFile("file:///repo/a.ts", "typescript", asView(second));
    first.dispatch({ changes: { from: 0, to: 3, insert: "first" } });
    second.dispatch({ changes: { from: 0, to: 3, insert: "second" } });

    expect(() => workspace.syncFiles()).toThrow(
      MedousaCodeWorkspaceConflictError,
    );
    expect(workspace.files[0].doc.toString()).toBe("one\n");
  });

  it("loads an unopened reference without translating through local paths", async () => {
    const { bridge, workspace, opened, closed } = createWorkspace();
    bridge.register({
      requestFile: (uri) =>
        uri === "file:///remote/repo/a%20b.ts"
          ? { languageId: "typescript", text: "export const value = 1;\n" }
          : null,
    });

    const file = await workspace.requestFile("file:///remote/repo/a b.ts#symbol");

    expect(file?.uri).toBe("file:///remote/repo/a%20b.ts");
    expect(file?.doc.toString()).toBe("export const value = 1;\n");
    expect(opened).toHaveLength(1);
    expect(closed).toEqual([]);
  });

  it("deduplicates concurrent requests for the same unopened file", async () => {
    const { bridge, workspace, opened } = createWorkspace();
    let loads = 0;
    bridge.register({
      requestFile: async () => {
        loads += 1;
        await Promise.resolve();
        return { languageId: "typescript", text: "export {};\n" };
      },
    });

    const [first, second] = await Promise.all([
      workspace.requestFile("file:///repo/a.ts"),
      workspace.requestFile("FILE:///repo/a.ts#reference"),
    ]);

    expect(first).toBe(second);
    expect(loads).toBe(1);
    expect(opened).toHaveLength(1);
    expect(workspace.files).toHaveLength(1);
  });

  it("lets the newest mounted Home surface reveal a requested document", async () => {
    const { bridge, workspace } = createWorkspace();
    const older = new TestView("older");
    const newer = new TestView("newer");
    const calls: string[] = [];
    bridge.register({
      displayFile: (uri) => {
        calls.push(`older:${uri}`);
        return asView(older);
      },
    });
    const unregister = bridge.register({
      displayFile: (uri) => {
        calls.push(`newer:${uri}`);
        return asView(newer);
      },
    });

    await expect(workspace.displayFile("file:///repo/a.ts")).resolves.toBe(
      asView(newer),
    );
    unregister();
    await expect(workspace.displayFile("file:///repo/a.ts")).resolves.toBe(
      asView(older),
    );
    expect(calls).toEqual([
      "newer:file:///repo/a.ts",
      "older:file:///repo/a.ts",
    ]);
  });

  it("canonicalizes equivalent workshop URI spellings", () => {
    expect(canonicalCodeDocumentUri("FILE:///Repo/a b.ts#symbol")).toBe(
      "file:///Repo/a%20b.ts",
    );
  });
});
