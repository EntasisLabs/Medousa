/** @vitest-environment happy-dom */
import { afterEach, expect, it } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { LSPClient } from "@codemirror/lsp-client";
import { setDiagnostics } from "@codemirror/lint";
import { codeEditorViewRegistry } from "$lib/code/codeEditorViewRegistry";
import CodeMirrorHost from "./CodeMirrorHost.svelte";

let component: ReturnType<typeof mount> | undefined;

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

it("clears a disconnected service's markers while preserving the editable draft", () => {
  const source = "export const answer = 42;\n";
  const uri = "file:///repo/app.ts";
  const state = $state<{ client: LSPClient | null }>({ client: new LSPClient() });
  const target = document.createElement("div");
  document.body.append(target);
  component = mount(CodeMirrorHost, { target, props: {
    value: source,
    languageId: "typescript",
    documentUri: uri,
    get client() { return state.client; },
  } });
  flushSync();
  const view = codeEditorViewRegistry.get(uri)!;
  expect(view).toBeTruthy();
  view.dispatch(setDiagnostics(view.state, [{
    from: 0, to: source.length, severity: "error", message: "obsolete provider error",
  }]));
  expect(component.getProblems()).toHaveLength(1);

  state.client = null;
  flushSync();
  expect(component.getProblems()).toHaveLength(0);
  expect(view.state.doc.toString()).toBe(source);
  component.insertText("// still editable\n");
  expect(view.state.doc.toString()).toContain("// still editable");
});

it("preserves provider, code, and precise ranges and clears observations on editing", async () => {
  const { presentCodeDiagnostics } = await import("$lib/code/codeDiagnosticPresentation");
  const client = new LSPClient();
  const uri = "file:///repo/precise.ts";
  component = mount(CodeMirrorHost, { target: document.body, props: { value: "const wrong = 1;\n", languageId: "typescript", documentUri: uri, client } });
  flushSync();
  const file = client.workspace.getFile(uri)!;
  expect(file).toBeTruthy();
  presentCodeDiagnostics(client, { uri, version: file.version, diagnostics: [{ message: "Wrong name", source: "typescript", code: 42, severity: 2, range: { start: { line: 0, character: 6 }, end: { line: 0, character: 11 } } }] });
  expect(component.getProblems()[0]).toMatchObject({ source: "typescript", code: 42, line: 1, character: 7, endLine: 1, endCharacter: 12 });
  component.revealProblemRange({ line: 1, character: 7, endLine: 1, endCharacter: 12 });
  const view = codeEditorViewRegistry.get(uri)!;
  expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("wrong");
  component.insertText("right");
  expect(component.getProblems()).toHaveLength(0);
  presentCodeDiagnostics(client, { uri, version: file.version - 1, diagnostics: [{ message: "obsolete", range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } } }] });
  expect(component.getProblems()).toHaveLength(0);
});
