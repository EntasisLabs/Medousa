import { EditorState, type TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import {
  navigateToCodeLanguageLocation,
  type CodeLanguageNavigationKind,
} from "./codeLanguageNavigation";

class NavigationView {
  state: EditorState;
  focused = false;
  pluginValue: unknown = null;

  constructor(doc: string, selection = 0) {
    this.state = EditorState.create({
      doc,
      selection: { anchor: selection },
    });
  }

  plugin() {
    return this.pluginValue;
  }

  dispatch(spec: TransactionSpec) {
    this.state = this.state.update(spec).state;
  }

  focus() {
    this.focused = true;
  }
}

function asView(view: NavigationView): EditorView {
  return view as unknown as EditorView;
}

function setup(
  response: unknown,
  options?: { capability?: boolean; target?: NavigationView | null },
) {
  const origin = new NavigationView("const value = target;\n", 14);
  const target = options?.target === undefined
    ? new NavigationView("export const target = 1;\n")
    : options.target;
  const calls: string[] = [];
  const client = {
    serverCapabilities: {
      definitionProvider: options?.capability ?? true,
      declarationProvider: options?.capability ?? true,
      typeDefinitionProvider: options?.capability ?? true,
      implementationProvider: options?.capability ?? true,
    },
    sync() {
      calls.push("sync");
    },
    request(method: string) {
      calls.push(method);
      return Promise.resolve(response);
    },
    workspace: {
      displayFile(uri: string) {
        calls.push(`display:${uri}`);
        return Promise.resolve(target ? asView(target) : null);
      },
    },
    withMapping(run: (mapping: unknown) => Promise<unknown>) {
      return run({
        getMapping: () => null,
        mapPosition: () => 0,
      });
    },
  };
  origin.pluginValue = {
    client,
    uri: "file:///repo/origin.ts",
    toPosition: () => ({ line: 0, character: 14 }),
    fromPosition: (position: { line: number; character: number }, doc: EditorState["doc"]) => {
      const line = doc.line(position.line + 1);
      return line.from + position.character;
    },
  };
  return { origin, target, calls };
}

describe("code language navigation", () => {
  it.each<[CodeLanguageNavigationKind, string]>([
    ["definition", "textDocument/definition"],
    ["declaration", "textDocument/declaration"],
    ["typeDefinition", "textDocument/typeDefinition"],
    ["implementation", "textDocument/implementation"],
  ])("requests and reveals %s", async (kind, method) => {
    const { origin, target, calls } = setup({
      uri: "file:///repo/target.ts",
      range: {
        start: { line: 0, character: 13 },
        end: { line: 0, character: 19 },
      },
    });

    await expect(
      navigateToCodeLanguageLocation(asView(origin), kind),
    ).resolves.toEqual({
      kind,
      uri: "file:///repo/target.ts",
      line: 1,
      character: 14,
      alternatives: 1,
    });
    expect(calls).toEqual([
      "sync",
      method,
      "display:file:///repo/target.ts",
    ]);
    expect(target?.state.selection.main.head).toBe(13);
    expect(target?.focused).toBe(true);
  });

  it("uses a LocationLink selection range and reports alternatives", async () => {
    const { origin, target } = setup([
      {
        targetUri: "file:///repo/target.ts",
        targetRange: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 24 },
        },
        targetSelectionRange: {
          start: { line: 0, character: 13 },
          end: { line: 0, character: 19 },
        },
      },
      {
        uri: "file:///repo/other.ts",
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 1 },
        },
      },
    ]);

    const result = await navigateToCodeLanguageLocation(
      asView(origin),
      "definition",
    );

    expect(result?.alternatives).toBe(2);
    expect(target?.state.selection.main.head).toBe(13);
  });

  it("does not request an explicitly unsupported capability", async () => {
    const { origin, calls } = setup(null, { capability: false });

    await expect(
      navigateToCodeLanguageLocation(asView(origin), "implementation"),
    ).resolves.toBe(null);
    expect(calls).toEqual([]);
  });

  it("returns null when Home cannot reveal any returned target", async () => {
    const { origin } = setup(
      {
        uri: "file:///outside/target.ts",
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 1 },
        },
      },
      { target: null },
    );

    await expect(
      navigateToCodeLanguageLocation(asView(origin), "definition"),
    ).resolves.toBe(null);
  });

  it("ignores malformed server locations", async () => {
    const { origin, calls } = setup([null, { uri: "file:///repo/no-range.ts" }]);

    await expect(
      navigateToCodeLanguageLocation(asView(origin), "definition"),
    ).resolves.toBe(null);
    expect(calls).toEqual(["sync", "textDocument/definition"]);
  });
});
