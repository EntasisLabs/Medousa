/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import Fixture from "./CodeMarkdownDocument.testfixture.svelte";
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; codeWorkspace.tabs = []; document.body.replaceChildren(); });
function open(content: string, options: { readOnly?: boolean; preview?: boolean } = {}) {
  codeWorkspace.tabs = [{ work_id: "work-a", path: "docs/README.md", tabId: "tab-a", title: "README.md", content, draft: content, digest: "digest", byte_size: content.length, language: "markdown", loading: false, error: null, syncKey: 0, line: null, preview: options.preview }];
  const onDraft = vi.fn();
  component = mount(Fixture, { target: document.body, props: { readOnly: options.readOnly, onDraft } });
  flushSync();
  return onDraft;
}
async function click(label: string) {
  const button = document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  expect(button).toBeTruthy();
  button.click(); await tick(); flushSync(); await tick();
}
it("keeps one unchanged draft and source editor instance when changing Markdown views", async () => {
  const original = "---\ntitle: 'Exact'\n---\n\n# Hello\n\n**One**\n";
  const onDraft = open(original);
  const sourceEditor = document.querySelector(".cm-editor");
  await click("Open in Markdown editor");
  expect(document.querySelector('[aria-label="Markdown editor"]')?.textContent).toContain("Hello");
  expect(onDraft).not.toHaveBeenCalled();
  expect(component!.getValue()).toBe(original);
  await click("Preview the current draft");
  expect(codeWorkspace.tabs[0].markdownMode).toBe("preview");
  expect(document.querySelector('[aria-label="Markdown preview"]')?.innerHTML).toContain("Hello");
  await click("Source and live Markdown preview");
  expect(document.querySelector(".cm-editor")).toBe(sourceEditor);
  expect(component!.getValue()).toBe(original);
});
it("rich edits update the exact buffer that Code's save controller reads", async () => {
  open("Hello world.\n");
  await click("Open in Markdown editor");
  const prose = document.querySelector<HTMLElement>('[aria-label="Markdown editor"]')!;
  // ProseMirror's DOM input path, rather than editing the shared store directly.
  prose.querySelector("p")!.textContent = "Hello changed world.";
  await new Promise((resolve) => setTimeout(resolve, 20));
  flushSync();
  expect(codeWorkspace.tabs[0].draft).toBe("Hello changed world.\n");
  expect(component!.getValue()).toBe(codeWorkspace.tabs[0].draft);
  await click("Edit Markdown source");
  expect(document.querySelector(".cm-content")?.textContent).toContain("changed world");
});
it("preserves read-only custody and does not offer rich modes for bounded source previews", async () => {
  open("Hello\n", { readOnly: true });
  await click("Open in Markdown editor");
  expect(document.querySelector('[aria-label="Markdown editor"]')?.getAttribute("contenteditable")).toBe("false");
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Bold"]')?.disabled).toBe(true);
  codeWorkspace.patch("tab-a", { preview: true }); flushSync();
  expect(document.querySelector('[aria-label="Markdown views"]')).toBeNull();
});

it("opens file Find from writing mode and keeps the same source buffer", async () => {
  open("Hello world.\n");
  await click("Open in Markdown editor");
  const prose = document.querySelector<HTMLElement>('[aria-label="Markdown editor"]')!;
  prose.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true, cancelable: true }));
  await tick(); flushSync(); await tick(); flushSync();
  expect(codeWorkspace.tabs[0].markdownMode).toBe("source");
  expect(document.querySelector('input[aria-label="Find in file"]')).toBeTruthy();
  expect(component!.getValue()).toBe("Hello world.\n");
});

it("opens relative project links from Preview in the same project", async () => {
  const onDraft = open("[Guide](../guide.md)\n");
  await click("Preview the current draft");
  document.querySelector<HTMLAnchorElement>('[aria-label="Markdown preview"] a')!.click();
  expect(onDraft).toHaveBeenCalledWith("open:guide.md");
});
