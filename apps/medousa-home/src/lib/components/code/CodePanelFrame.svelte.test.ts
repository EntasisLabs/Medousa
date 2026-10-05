/** @vitest-environment happy-dom */
import { afterEach, expect, it } from "vitest";
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import CodePanelFrame from "./CodePanelFrame.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); localStorage.clear(); });
it("resizes with the keyboard and restores the size when reopened", async () => {
  const children = createRawSnippet(() => ({ render: () => '<div>Panel content</div>' }));
  component = mount(CodePanelFrame, { target: document.body, props: { workId: "work", name: "Tests", children } }); flushSync();
  const slider = document.querySelector<HTMLElement>('[role="slider"]')!;
  slider.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true })); flushSync();
  expect(slider.getAttribute("aria-valuenow")).toBe("344");
  await unmount(component);
  component = mount(CodePanelFrame, { target: document.body, props: { workId: "work", name: "Tests", children } }); flushSync();
  expect(document.querySelector('[role="slider"]')?.getAttribute("aria-valuenow")).toBe("344");
});
