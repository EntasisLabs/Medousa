/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
vi.mock("$lib/components/terminal/TerminalPane.svelte", () => ({ default: () => {} }));
import CodeTerminalDock from "./CodeTerminalDock.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });
it("opening an empty dock requires an explicit shell action", () => {
  const create = vi.fn();
  component = mount(CodeTerminalDock, { target: document.body, props: { open: true, workId: "work", sessionId: null, canCreateTerminal: true, onCreate: create, onClose: vi.fn() } });
  flushSync();
  expect(create).not.toHaveBeenCalled();
  const button = Array.from(document.querySelectorAll("button")).find((el) => el.textContent?.includes("Create shell"))!;
  button.click();
  expect(create).toHaveBeenCalledOnce();
});
it("keeps blocked errors readable and raw responses in Details", () => {
  component = mount(CodeTerminalDock, { target: document.body, props: { open: true, workId: "work", sessionId: null, error: 'HTTP 409: {"error":"attached checkout switched branches: expected old, found new"}', canCreateTerminal: false, onCreate: vi.fn(), onClose: vi.fn() } });
  flushSync();
  expect(document.body.textContent).toContain("working copy changed branches");
  expect(document.querySelector("details")?.textContent).toContain("HTTP 409");
  expect(document.querySelector<HTMLButtonElement>("button")?.disabled).toBe(true);
});
