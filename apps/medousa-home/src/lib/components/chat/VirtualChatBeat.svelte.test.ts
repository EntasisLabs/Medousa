/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { createRawSnippet, flushSync, mount, tick, unmount } from "svelte";
import VirtualChatBeat from "./VirtualChatBeat.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

async function fixture(keepMounted = false) {
  let notify!: IntersectionObserverCallback;
  const disconnected = vi.fn();
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback) { notify = callback; }
    observe() {}
    disconnect = disconnected;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ height: 120, width: 400, top: 0, bottom: 120, left: 0, right: 400, x: 0, y: 0, toJSON: () => ({}) });
  const root = document.createElement("div"); root.className = "chat-scroll"; document.body.append(root);
  const cleanup = vi.fn();
  const children = createRawSnippet(() => ({ render: () => '<div class="rich-turn">Rendered turn<details><summary>Tools</summary>Results</details><div class="markdown-code-block"><pre><code>example</code></pre></div></div>', setup: () => cleanup }));
  component = mount(VirtualChatBeat, { target: root, props: { children, userId: "user", keepMounted } });
  flushSync(); await tick();
  return { root, cleanup, disconnected, enter: async (isIntersecting: boolean) => { notify([{ isIntersecting } as IntersectionObserverEntry], {} as IntersectionObserver); flushSync(); await tick(); } };
}

it("mounts only nearby rich turns and preserves their measured scroll space after releasing them", async () => {
  const f = await fixture();
  expect(f.root.querySelector(".rich-turn")).toBeNull();
  await f.enter(true);
  expect(f.root.querySelector(".rich-turn")).not.toBeNull();
  await f.enter(false);
  expect(f.root.querySelector(".rich-turn")).toBeNull();
  expect(f.cleanup).toHaveBeenCalledTimes(1);
  expect((f.root.querySelector(".virtual-chat-beat") as HTMLElement).style.height).toBe("120px");
  expect(f.root.querySelector('[data-chat-turn-user-id="user"]')).not.toBeNull();
  await f.enter(true);
  expect(f.root.querySelector(".rich-turn")).not.toBeNull();
});

it("keeps the active streaming turn mounted and disconnects observers on tab teardown", async () => {
  const f = await fixture(true);
  await f.enter(false);
  expect(f.root.querySelector(".rich-turn")).not.toBeNull();
  expect(f.cleanup).not.toHaveBeenCalled();
  await unmount(component!); component = undefined;
  expect(f.disconnected).toHaveBeenCalledTimes(1);
  expect(f.cleanup).toHaveBeenCalledTimes(1);
});


it("preserves folded details and code display choices without retaining the rendered body", async () => {
  const f = await fixture();
  await f.enter(true);
  (f.root.querySelector("details") as HTMLDetailsElement).open = true;
  const block = f.root.querySelector(".markdown-code-block") as HTMLElement;
  block.dataset.codeWrapped = "true"; block.dataset.codeExpanded = "true";
  await f.enter(false);
  expect(f.root.querySelector("details")).toBeNull();
  await f.enter(true);
  expect((f.root.querySelector("details") as HTMLDetailsElement).open).toBe(true);
  const restored = f.root.querySelector(".markdown-code-block") as HTMLElement;
  expect(restored.dataset.codeWrapped).toBe("true");
  expect(restored.dataset.codeExpanded).toBe("true");
  expect(restored.classList.contains("markdown-code-wrapped")).toBe(true);
});
