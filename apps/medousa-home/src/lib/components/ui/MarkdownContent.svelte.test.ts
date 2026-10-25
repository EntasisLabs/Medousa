/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import MarkdownContent from "./MarkdownContent.svelte";

const { highlight, writeText, openInBrowser, openCreative } = vi.hoisted(() => ({
  openCreative: vi.fn().mockResolvedValue(undefined),
  highlight: vi.fn().mockResolvedValue(undefined),
  writeText: vi.fn().mockResolvedValue(undefined),
  openInBrowser: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("$lib/utils/openInBrowser", async (importOriginal) => ({
  ...await importOriginal<typeof import("$lib/utils/openInBrowser")>(),
  openInBrowser,
}));
vi.mock("$lib/utils/openCreativeResult", () => ({ openCreativeResult: openCreative }));
vi.mock("$lib/markdown/highlight", () => ({ highlightCodeBlocks: highlight }));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
// Happy DOM's parser drops the outer wrapper during DOMPurify sanitization.
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  document.body.replaceChildren();
});

it("copies the live streaming tail, subsequent chunks, and the completed block", async () => {
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const store = writable({ content: "```sh\n  echo first", streaming: true });
  const state = fromStore(store);
  component = mount(MarkdownContent, {
    target: document.body,
    props: {
      get content() { return state.current.content; },
      get streaming() { return state.current.streaming; },
    },
  });
  flushSync();
  const clickCopy = async (source: string) => {
    const button = document.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
    button.click();
    await vi.waitFor(() => expect(writeText).toHaveBeenLastCalledWith(source));
    await vi.waitFor(() => expect(button.textContent).toBe("Copied"));
  };
  expect(document.querySelector("[data-streaming-markdown-tail] code")).not.toBeNull();
  await clickCopy("  echo first");
  expect(highlight).not.toHaveBeenCalled();

  store.set({ content: "```sh\n  echo first\n  echo second", streaming: true });
  await tick();
  expect(document.querySelector("code")?.textContent).toBe("  echo first\n  echo second");
  await clickCopy("  echo first\n  echo second");
  expect(highlight).not.toHaveBeenCalled();

  store.set({ content: "```sh\n  echo first\n  echo second\n```", streaming: false });
  await tick();
  expect(document.querySelector("[data-streaming-markdown-tail]")).toBeNull();
  await clickCopy("  echo first\n  echo second");
  expect(writeText).toHaveBeenCalledTimes(3);
});

it("copies completed history loaded into an already mounted nonstreaming view", async () => {
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const store = writable("");
  const state = fromStore(store);
  component = mount(MarkdownContent, {
    target: document.body,
    props: { get content() { return state.current; } },
  });
  flushSync();
  store.set("```sh\n  echo history\n```");
  await tick();
  const button = document.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
  button.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith("  echo history"));
  await vi.waitFor(() => expect(button.textContent).toBe("Copied"));
});

it("uses Liquid disclosure styling and real linked badges in document presentation", async () => {
  component = mount(MarkdownContent, {
    target: document.body,
    props: { presentation: "document", content: '<details>\n<summary>Release notes</summary>\n\n**Fixed** the issue.\n\n</details>\n\n[![Build status](https://example.com/status.svg)](https://example.com/build)' },
  });
  flushSync(); await tick();
  expect(document.querySelector<HTMLDetailsElement>(".markdown-document .liquid-accordion details")?.open).toBe(true);
  expect(document.querySelector(".liquid-accordion-panel strong")?.textContent).toBe("Fixed");
  expect(document.querySelector("a .markdown-image-inline img")?.getAttribute("src")).toBe("https://example.com/status.svg");
  expect(document.body.textContent).not.toContain("![Build status]");
});

it.each([
  ["click", {}],
  ["click", { metaKey: true }],
  ["click", { ctrlKey: true }],
  ["click", { shiftKey: true }],
  ["auxclick", { button: 1 }],
])("opens nested review links in an app tab and cancels native %s navigation (%o)", async (type, options) => {
  component = mount(MarkdownContent, {
    target: document.body,
    props: { openLinksInWeb: true, presentation: "document", content: '<details open><summary>Notes</summary>\n\n[![Build](https://example.com/badge.svg)](https://example.com/build)\n\n</details>' },
  });
  flushSync(); await tick();
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, ...options });
  document.querySelector("a img")!.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(openInBrowser).toHaveBeenCalledExactlyOnceWith("https://example.com/build", { openedBy: "user", newTab: true });
});

it("resolves raw relative review links against the provider page and preserves local anchors", async () => {
  component = mount(MarkdownContent, {
    target: document.body,
    props: { openLinksInWeb: true, linkBaseUrl: "https://git.example.test/team/repo/pull/4", content: '<a href="/team/repo/issues/7"><strong>Issue</strong></a>\n\n[Section](#section)' },
  });
  flushSync(); await tick();
  const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  document.querySelector("a strong")!.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(openInBrowser).toHaveBeenCalledExactlyOnceWith("https://git.example.test/team/repo/issues/7", { openedBy: "user", newTab: true });
  const anchorEvent = new MouseEvent("click", { bubbles: true, cancelable: true });
  document.querySelector('a[href="#section"]')!.dispatchEvent(anchorEvent);
  expect(anchorEvent.defaultPrevented).toBe(false);
  expect(openInBrowser).toHaveBeenCalledTimes(1);
});

it("opens a validated Create result from a conversation without navigating the webview", async () => {
  component = mount(MarkdownContent, { target: document.body, props: { content: "[Open wireframe](medousa://create/wireframe_123)" } });
  flushSync(); await tick();
  const event = new MouseEvent("click", { bubbles: true, cancelable: true });
  document.querySelector("a")!.dispatchEvent(event);
  await vi.waitFor(() => expect(openCreative).toHaveBeenCalledWith("wireframe_123"));
  expect(event.defaultPrevented).toBe(true); expect(openInBrowser).not.toHaveBeenCalled();
});
