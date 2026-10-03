/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { createNode } from "$lib/liquid/core";
import { hydrateCodeBlocks } from "$lib/markdown/codeBlocks";
import Code from "./Code.svelte";

vi.mock("$lib/syntax/highlightCode", () => ({
  highlightElement: vi.fn().mockRejectedValue(new Error("unavailable")),
  highlightCodeBlocks: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));

let component: ReturnType<typeof mount> | undefined;
const writeText = vi.fn();
beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
});
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});
function open(source: string, props: Record<string, unknown> = {}) {
  component = mount(Code, {
    target: document.body,
    props: { node: createNode({ id: "snippet", type: "code", props: { source, lang: "ts", ...props }, fillState: "ready" }) },
  });
  flushSync();
}
async function click(selector: string) {
  document.querySelector<HTMLButtonElement>(selector)!.click();
  await tick();
  flushSync();
}

it("keeps Liquid controls working independently of generic Markdown hydration", async () => {
  const source = Array.from({ length: 20 }, (_, i) => `const value${i} = ${i};`).join("\n");
  open(source);
  await hydrateCodeBlocks(document.body);
  expect(document.querySelector(".markdown-code-lang")?.textContent).toBe("TypeScript");
  expect(document.querySelector("code")?.textContent).toBe(source);
  await click(".markdown-code-wrap");
  expect(document.querySelector(".markdown-code-wrap")?.getAttribute("aria-pressed")).toBe("true");
  await click(".markdown-code-expand");
  expect(document.querySelector(".markdown-code-collapsed")).toBeNull();
  await click(".markdown-code-copy");
  expect(writeText).toHaveBeenCalledExactlyOnceWith(source);
  expect(document.querySelector(".markdown-code-copy")?.textContent).toBe("Copied");
});

it("honors disabled Copy and copies diff source exactly when enabled", async () => {
  const source = "-old\n+new";
  open(source, { lang: "diff", copy: false });
  await hydrateCodeBlocks(document.body);
  expect(document.querySelector(".markdown-code-copy")).toBeNull();
  await unmount(component!);
  open(source, { lang: "diff" });
  expect(document.querySelector("code")?.textContent).toBe(source);
  await click(".markdown-code-copy");
  expect(writeText).toHaveBeenCalledExactlyOnceWith(source);
});
