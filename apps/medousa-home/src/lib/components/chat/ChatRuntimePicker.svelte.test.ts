/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { AccountConnectionInfo } from "$lib/utils/accountConnections";
const api = vi.hoisted(() => ({ info: {} as Partial<AccountConnectionInfo>, refresh: vi.fn() }));
vi.mock("$lib/stores/accountConnections.svelte", () => ({ accountConnections: { refresh: api.refresh, connection: () => api.info } }));
vi.mock("$lib/stores/settings.svelte", () => ({ settings: { medousaMark: "default", darkMode: true } }));
vi.mock("$lib/stores/settingsNav.svelte", () => ({ settingsNav: { setActiveSection: vi.fn() } }));
vi.mock("$lib/components/brand/MedousaMark.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/brand/ExternalAgentLogo.svelte", () => ({ default: () => {} }));
import ChatRuntimePicker from "./ChatRuntimePicker.svelte";
let component: ReturnType<typeof mount> | undefined;
beforeEach(() => { vi.clearAllMocks(); api.info = { available: true, binaryPresent: true, authStatus: "unknown" }; });
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });
function render() {
  const change = vi.fn(); const setup = vi.fn();
  component = mount(ChatRuntimePicker, { target: document.body, props: { inline: true, value: "medousa", onChange: change, onSettings: setup } });
  flushSync();
  return { change, setup };
}
function click(label: string) {
  const button = [...document.querySelectorAll("button")].find(item => item.querySelector(".chat-runtime-option-title")?.textContent?.trim() === label)!;
  button.click(); flushSync(); return button;
}
it("allows available agents whose authentication cannot be preverified", () => {
  const { change, setup } = render();
  const button = click("Claude Code");
  expect(button.getAttribute("aria-disabled")).toBe("false");
  expect(change).toHaveBeenCalledWith("claude"); expect(setup).not.toHaveBeenCalled();
});
it("routes known signed-out agents to setup", () => {
  api.info.authStatus = "signed_out";
  const { change, setup } = render(); click("Cursor");
  expect(change).not.toHaveBeenCalled(); expect(setup).toHaveBeenCalledOnce();
});
it("does not treat a native binary as a usable runtime when its bridge or platform is unavailable", () => {
  api.info.available = false; api.info.detail = "Hades requires a Linux workshop";
  const { change, setup } = render(); click("Hades");
  expect(change).not.toHaveBeenCalled(); expect(setup).toHaveBeenCalledOnce();
  expect(document.body.textContent).toContain("Hades requires a Linux workshop");
});
