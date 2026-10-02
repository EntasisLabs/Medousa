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
