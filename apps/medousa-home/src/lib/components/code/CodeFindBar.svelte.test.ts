/** @vitest-environment happy-dom */
import { afterEach, expect, it } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { undo } from "@codemirror/commands";
import { getSearchQuery, searchPanelOpen } from "@codemirror/search";
import { CodeFindState, CodeFindStateCache } from "$lib/code/codeFindController.svelte";
import CodeMirrorHost from "./CodeMirrorHost.svelte";

let component: ReturnType<typeof mount> | undefined;

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

function editor(value = "alpha Alpha alphabet\nalpha\n", state = new CodeFindState(), readOnly = false) {
  component = mount(CodeMirrorHost, { target: document.body, props: { value, languageId: "plaintext", findState: state, readOnly } });
  flushSync();
  return { host: component, view: component.getView()!, state };
}

function input(label: string, value: string) {
  const field = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  return field;
}

function button(label: string) {
  const element = document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  expect(element).toBeTruthy();
  element.click();
  flushSync();
  return element;
}

function key(field: HTMLElement, key: string, options: KeyboardEventInit = {}) {
  field.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options }));
  flushSync();
}

it("opens floating Find, keeps focus while navigating, and restores editor focus on Escape", () => {
  const { host, view } = editor();
  host.openFind();
  flushSync();
  const query = document.querySelector<HTMLInputElement>('input[aria-label="Find in file"]')!;
  expect(document.activeElement).toBe(query);
  expect(document.querySelector('.cm-search')).toBeNull();
  expect(searchPanelOpen(view.state)).toBe(true);
  expect(document.querySelector('input[aria-label="Replace with"]')).toBeNull();
  input("Find in file", "alpha");
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("1 of 4");
  key(query, "Enter");
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("2 of 4");
  expect(document.activeElement).toBe(query);
  key(query, "Enter", { shiftKey: true });
  expect(view.state.selection.main.from).toBe(0);
  key(query, "Enter", { shiftKey: true });
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("4 of 4");
  key(query, "Escape");
  expect(document.querySelector('[role="search"]')).toBeNull();
  expect(searchPanelOpen(view.state)).toBe(false);
  expect(document.activeElement).toBe(view.contentDOM);
  expect(document.querySelector('.cm-searchMatch')).toBeNull();
});

it("keeps the current hit when extending a query and reports no results without changing the buffer", () => {
  const { host, view } = editor("apple apricot apple");
  host.openFind(); flushSync();
  input("Find in file", "a");
  input("Find in file", "ap");
  expect(view.state.selection.main.from).toBe(0);
  input("Find in file", "not here");
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("No results");
  expect(document.querySelector<HTMLButtonElement>('button[aria-label="Next match"]')?.disabled).toBe(true);
  expect(view.state.doc.toString()).toBe("apple apricot apple");
});

it("supports case, whole-word, valid and invalid regex, capture replacements, and a single undo", async () => {
  const { host, view } = editor();
  host.openFind(); flushSync();
  input("Find in file", "alpha");
  button("Match case"); button("Whole word");
  expect(document.querySelector('.editor-find-status')?.textContent).toContain("2");
  button("Regular expression");
  input("Find in file", "[");
  expect(document.querySelector('[aria-invalid="true"]')).toBeTruthy();
  expect(document.querySelector('[role="status"]')?.textContent).toContain("Invalid regular expression");
  expect(document.querySelector<HTMLButtonElement>('button[aria-label="Next match"]')?.disabled).toBe(true);
  input("Find in file", "(alpha)");
  button("Toggle replace");
  await Promise.resolve(); flushSync();
  input("Replace with", "$1_new");
  const replacement = document.querySelector<HTMLInputElement>('input[aria-label="Replace with"]')!;
  expect(document.activeElement).toBe(replacement);
  key(replacement, "Enter", { ctrlKey: true });
  expect(view.state.doc.toString()).toBe("alpha_new Alpha alphabet\nalpha_new\n");
  expect(document.querySelector('[role="status"]')?.textContent).toContain("Replaced 2 matches");
  const undoButton = [...document.querySelectorAll('button')].find((b) => b.textContent === "Undo")!;
  undoButton.click(); flushSync();
  expect(view.state.doc.toString()).toBe("alpha Alpha alphabet\nalpha\n");
  expect(document.querySelector('[role="status"]')?.textContent).toContain("Replacement undone");
  expect(undo(view)).toBe(false);
});

it("limits replacement to the original selection and maps its bounds through edits and undo", () => {
  const { host, view, state } = editor("cat cat\ncat");
  view.dispatch({ selection: { anchor: 0, head: 7 } });
  host.openFind(); flushSync();
  input("Find in file", "cat");
  button("Find in selection");
  expect(state.selection).toEqual({ from: 0, to: 7 });
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("1 of 2");
  button("Toggle replace");
  input("Replace with", "kitten");
  const replacement = document.querySelector<HTMLInputElement>('input[aria-label="Replace with"]')!;
  key(replacement, "Enter", { ctrlKey: true });
  expect(view.state.doc.toString()).toBe("kitten kitten\ncat");
  expect(state.selection).toEqual({ from: 0, to: 13 });
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("No results");
  undo(view); flushSync();
  expect(state.selection).toEqual({ from: 0, to: 7 });
  expect(document.querySelector('.editor-find-status')?.textContent).toContain("2");
  view.dispatch({ changes: { from: 0, insert: "cat " } }); flushSync();
  expect(state.selection).toEqual({ from: 0, to: 11 });
  expect(getSearchQuery(view.state).getCursor(view.state).next().value).toMatchObject({ from: 0, to: 3 });
});

it("keeps Find available in a read-only preview and never exposes replacement controls", () => {
  const { host, view } = editor("one one", new CodeFindState(), true);
  host.openReplace(); flushSync();
  input("Find in file", "one");
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("1 of 2");
  expect(document.querySelector('button[aria-label="Toggle replace"]')).toBeNull();
  expect(document.querySelector('input[aria-label="Replace with"]')).toBeNull();
  expect(document.querySelector('.editor-find-message')?.textContent).toBe("Read-only file");
  expect(view.state.doc.toString()).toBe("one one");
});

it("reports bounded counts honestly and can still replace a selected match beyond the counted prefix", () => {
  const { host, view } = editor("a ".repeat(10_001));
  host.openFind(); flushSync(); input("Find in file", "a");
  expect(document.querySelector('.editor-find-status')?.textContent).toBe("10,000+ matches");
  view.dispatch({ selection: { anchor: 20_000, head: 20_001 }, userEvent: "select.search" });
  button("Toggle replace"); input("Replace with", "b");
  key(document.querySelector<HTMLInputElement>('input[aria-label="Replace with"]')!, "Enter");
  expect(view.state.doc.toString()).toBe("a ".repeat(10_000) + "b ");
});

it("restores file-local query/options/open state across remounts without sharing them with another file", async () => {
  const cache = new CodeFindStateCache();
  const a = cache.forFile("project", "a", ["a", "b"])!;
  const { host } = editor("one one", a);
  host.openFind(); flushSync(); input("Find in file", "one"); button("Match case"); button("Toggle replace");
  input("Replace with", "two");
  await unmount(component!); component = undefined;
  const b = cache.forFile("project", "b", ["a", "b"])!;
  editor("other", b);
  expect(document.querySelector('[role="search"]')).toBeNull();
  expect(b.query).toBe("");
  await unmount(component!); component = undefined;
  editor("one one", cache.forFile("project", "a", ["a", "b"])!);
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Find in file"]')?.value).toBe("one");
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Replace with"]')?.value).toBe("two");
  expect(document.querySelector('button[aria-label="Match case"]')?.getAttribute("aria-pressed")).toBe("true");
  expect(cache.forFile("different project", "a", ["a"])!.open).toBe(false);
});

it("honors parent-owned mobile Find and keeps project-search shortcuts distinct", () => {
  let requested = 0;
  component = mount(CodeMirrorHost, { target: document.body, props: { value: "alpha", onFindRequested: () => requested++ } });
  flushSync();
  component.openFind(); flushSync();
  expect(requested).toBe(1);
  expect(document.querySelector('[role="search"]')).toBeNull();
  expect(searchPanelOpen(component.getView()!.state)).toBe(false);
  key(component.getView()!.contentDOM, "F", { ctrlKey: true, shiftKey: true, keyCode: 70 });
  expect(requested).toBe(1);
});
