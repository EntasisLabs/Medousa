/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
import type { ChatMessage } from "$lib/types/chat";

const { highlight, writeText } = vi.hoisted(() => ({
  highlight: vi.fn<(root: HTMLElement) => Promise<void>>().mockResolvedValue(undefined),
  writeText: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("$lib/markdown/highlight", () => ({ highlightCodeBlocks: highlight }));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  document.body.replaceChildren();
});

function message(source: string, streaming = true, committed = false): ChatMessage {
  return {
    id: "stream", role: "assistant", content: source, streaming,
    segments: [{ kind: "text", segmentId: "answer", modelRound: 1, markdown: source, committed }],
  };
}

async function fixture(initial: ChatMessage) {
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const store = writable(initial);
  const state = fromStore(store);
  component = mount(LiquidChatMessage, {
    target: document.body,
    props: { get message() { return state.current; }, sessionId: "stream-session" },
  });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector(".liquid-prose .markdown-content")).not.toBeNull());
  return store;
}

describe("streaming through the actual Liquid chat path", () => {
  it.each(["legacy", "segments"])("retains completed %s blocks and code controls across chunks and completion", async (mode) => {
    const code = Array.from({ length: 15 }, (_, i) => `echo line${i}`).join("\n");
    const prefix = `## Heading\n\nFirst paragraph.\n\n\`\`\`sh\n${code}\n\`\`\`\n\nNext paragraph.\n\nLast paragraph`;
    const build = (source: string, streaming = true) => {
      const msg = message(source, streaming);
      if (mode === "legacy") delete msg.segments;
      return msg;
    };
    const store = await fixture(build(prefix));
    await vi.waitFor(() => expect(document.querySelector("[data-stable-markdown-block] code")).not.toBeNull());
    const heading = document.querySelector("h2")!;
    const block = document.querySelector<HTMLElement>(".markdown-code-block")!;
    const copy = block.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
    copy.click();
    await vi.waitFor(() => expect(copy.textContent).toBe("Copied"));
    block.querySelector<HTMLButtonElement>(".markdown-code-wrap")!.click();
    block.querySelector<HTMLButtonElement>(".markdown-code-expand")!.click();
    const codeHydrations = () => highlight.mock.calls.filter(([root]) => root.contains(block)).length;
    const hydrations = codeHydrations();
    for (const suffix of [" grows", " grows further", " grows further.\n\nAnother paragraph."]) {
      store.set(build(prefix + suffix));
      await tick();
      expect(document.querySelector("h2")).toBe(heading);
      expect(document.querySelector(".markdown-code-block")).toBe(block);
      expect(block.classList.contains("markdown-code-wrapped")).toBe(true);
      expect(block.classList.contains("markdown-code-collapsed")).toBe(false);
      expect(copy.textContent).toBe("Copied");
    }
    expect(codeHydrations()).toBe(hydrations);
    store.set(build(prefix + " grows further.\n\nAnother paragraph.", false));
    await tick();
    expect(document.querySelector("h2")).toBe(heading);
    expect(document.querySelector(".markdown-code-block")).toBe(block);
    expect(document.querySelector("[data-streaming-markdown-tail]")).toBeNull();
    expect(block.classList.contains("markdown-code-wrapped")).toBe(true);
  });

  it("carries explicit code choices through mutable updates and promotion into a completed block", async () => {
    const code = Array.from({ length: 14 }, (_, i) => `echo line${i}`).join("\n");
    const start = `\`\`\`sh\n${code}`;
    const store = await fixture(message(start));
    document.querySelector<HTMLButtonElement>(".markdown-code-expand")!.click();
    document.querySelector<HTMLButtonElement>(".markdown-code-wrap")!.click();
    for (const suffix of ["\necho new", "\necho new\n```\n\nAfter.\n\nLast."]) {
      store.set(message(start + suffix));
      await tick();
      expect(document.querySelector(".markdown-code-wrapped")).not.toBeNull();
      expect(document.querySelector(".markdown-code-collapsed")).toBeNull();
    }
    store.set(message(start + "\necho new\n```\n\nAfter.\n\nLast.", false));
    await tick();
    expect(document.querySelector(".markdown-code-wrapped")).not.toBeNull();
    expect(document.querySelector(".markdown-code-collapsed")).toBeNull();
  });

  it("keeps list nesting, table columns and a canonical replacement correct", async () => {
    const source = "# Start\n\n- parent\n  - child\n\n| Role | Address |\n| --- | --- |\n| Server | 10.12.0.11 |\n\nLast";
    const store = await fixture(message(source));
    for (let end = source.length; end <= source.length + 10; end += 2) {
      store.set(message(source + " grows more".slice(0, end - source.length)));
      await tick();
      expect(document.querySelector("ul > li > ul > li")?.textContent).toContain("child");
      expect(document.querySelectorAll("thead th")).toHaveLength(2);
      expect(document.querySelector("tbody td:last-child")?.textContent).toBe("10.12.0.11");
    }
    store.set(message("## Corrected\n\nCanonical answer", false, true));
    await tick();
    expect(document.querySelector("h2")?.textContent).toBe("Corrected");
    expect(document.querySelector("h1")).toBeNull();
    expect(document.querySelector("table")).toBeNull();
  });

  it("retains the first response through tool activity, a second text segment and turn completion", async () => {
    const source = "## Inspecting\n\nFirst paragraph.\n\nSecond paragraph.\n\nLast paragraph.";
    const store = await fixture(message(source));
    const heading = document.querySelector("h2")!;
    const initial = message(source);
    const text = initial.segments![0];
    if (text.kind !== "text") throw new Error("Expected text fixture");
    for (const status of ["running", "succeeded"] as const) {
      store.set({
        ...initial,
        segments: [
          { ...text, committed: true },
          { kind: "tool_group", groupId: "tools", toolRound: 1, runs: [{ runId: "inspect", toolName: "cognition_code_read", status, round: 1 }] },
        ],
      });
      await tick();
      expect(document.querySelector("h2")).toBe(heading);
    }
    const next: ChatMessage = {
      ...initial, content: `${source}\n\nThe result is ready.`,
      segments: [
        { ...text, committed: true },
        { kind: "tool_group", groupId: "tools", toolRound: 1, runs: [{ runId: "inspect", toolName: "cognition_code_read", status: "succeeded", round: 1 }] },
        { kind: "text", segmentId: "result", modelRound: 2, committed: false, markdown: "The result is ready." },
      ],
    };
    store.set(next);
    await vi.waitFor(() => expect(document.body.textContent).toContain("The result is ready."));
    store.set({ ...next, streaming: false });
    await tick();
    expect(document.querySelector("h2")).toBe(heading);
    expect(document.querySelectorAll("h2")).toHaveLength(1);
    expect(document.body.textContent).toContain("The result is ready.");
  });
});
