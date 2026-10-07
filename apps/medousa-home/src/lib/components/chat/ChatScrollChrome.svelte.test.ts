/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import type { ChatMessage } from "$lib/types/chat";
import ChatStreamingHarness from "./ChatStreamingHarness.svelte";

vi.mock("$lib/markdown/highlight", () => ({ highlightCodeBlocks: vi.fn().mockResolvedValue(undefined) }));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

it.each([false, true])("retains rendered chat while layout follow respects reading intent (mobile=%s)", async (mobile) => {
  const callbacks: (() => void)[] = [];
  const disconnect = vi.fn();
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { callbacks.push(callback); }
    observe = vi.fn();
    disconnect = disconnect;
  });
  let frameId = 0;
  const frames = new Map<number, FrameRequestCallback>();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++frameId, callback);
    return frameId;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  const frame = () => {
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach((callback) => callback(0));
  };
  const build = (content: string, streaming = true): ChatMessage => ({
    id: "response", role: "assistant", content, streaming,
    segments: [{ kind: "text", segmentId: "answer", modelRound: 1, committed: !streaming, markdown: content }],
  });
  const source = "# Stable heading\n\nFirst paragraph.\n\nSecond paragraph.\n\nLive tail";
  const store = writable(build(source));
  const state = fromStore(store);
  component = mount(ChatStreamingHarness, {
    target: document.body,
    props: { get message() { return state.current; }, mobile },
  });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector("[data-stable-markdown-block] h1")).not.toBeNull());
  const heading = document.querySelector("h1");
  const root = document.querySelector<HTMLElement>(".chat-scroll")!;
  let height = 1_000;
  Object.defineProperties(root, {
    scrollHeight: { get: () => height }, clientHeight: { get: () => 300 },
  });
  root.scrollTop = 700;
  const scrollTo = vi.fn((options: ScrollToOptions) => { root.scrollTop = options.top ?? root.scrollTop; });
  Object.defineProperty(root, "scrollTo", { value: scrollTo });
  frame();
  store.set(build(source + " grows"));
  await tick();
  height = 1_100;
  callbacks.forEach((callback) => callback());
  callbacks.forEach((callback) => callback());
  frame();
  expect(scrollTo).toHaveBeenCalledExactlyOnceWith({ top: 800, behavior: "auto" });
  expect(document.querySelector("h1")).toBe(heading);

  root.dispatchEvent(new WheelEvent("wheel", { deltaY: -20 }));
  root.scrollTop = 600;
  root.dispatchEvent(new Event("scroll"));
  await tick();
  expect(document.querySelector(".chat-scroll-fab")).not.toBeNull();
  store.set(build(source + " grows further", false));
  await tick();
  height = 1_200;
  callbacks.forEach((callback) => callback());
  frame();
  expect(root.scrollTop).toBe(600);
  expect(scrollTo).toHaveBeenCalledTimes(1);
  expect(document.querySelector("h1")).toBe(heading);
  expect(document.querySelector("[data-streaming-markdown-tail]")).toBeNull();
  await unmount(component);
  component = undefined;
  expect(disconnect).toHaveBeenCalledTimes(1);
  expect(frames.size).toBe(0);
});
