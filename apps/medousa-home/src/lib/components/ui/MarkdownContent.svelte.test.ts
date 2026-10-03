/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import MarkdownContent from "./MarkdownContent.svelte";

const { highlight, writeText } = vi.hoisted(() => ({
  highlight: vi.fn().mockResolvedValue(undefined),
  writeText: vi.fn().mockResolvedValue(undefined),
}));
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
