/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
import SettingsSshSection from "./SettingsSshSection.svelte";

const api = vi.hoisted(() => ({
  targets: vi.fn(), action: vi.fn(), access: vi.fn(), remove: vi.fn(), terminal: vi.fn(), navigate: vi.fn(),
}));
vi.mock("$lib/utils/sshApi", () => ({ sshTargets: api.targets, sshAction: api.action, sshSetAccess: api.access, sshRemove: api.remove }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openTerminal: api.terminal } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { navigateDesktop: api.navigate } }));
vi.mock("$lib/platform", () => ({ isTauriDesktop: () => true }));
let component: ReturnType<typeof mount> | undefined;
async function settle() { await new Promise((resolve) => setTimeout(resolve, 0)); flushSync(); }
function button(label: string): HTMLButtonElement {
  return [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(label))!;
}
function input(label: string): HTMLInputElement {
  return [...document.querySelectorAll("label")].find((l) => l.textContent?.trim().startsWith(label))!.querySelector("input")!;
}
function type(label: string, value: string) {
  const el = input(label); el.value = value; el.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
}
beforeEach(() => { vi.resetAllMocks(); api.targets.mockResolvedValue([]); });
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it("requires explicit server trust and invalidates the fingerprint when the endpoint changes", async () => {
  api.action.mockResolvedValue({ host_keys: ["ssh-ed25519 public"], fingerprints: ["ssh-ed25519 SHA256:fingerprint"] });
  component = mount(SettingsSshSection, { target: document.body }); await settle();
  button("Add server").click(); flushSync();
  type("Name", "NAS"); type("Host", "nas.local"); type("Username", "ops");
  button("Check server identity").click(); await settle();
  expect(button("Save connection").disabled).toBe(true);
  input("Trust this server identity").click(); flushSync();
  expect(button("Save connection").disabled).toBe(false);
  type("Host", "different.local");
  expect([...document.querySelectorAll("button")].some((b) => b.textContent?.includes("Save connection"))).toBe(false);
  expect(api.action).toHaveBeenCalledTimes(1);
});

it("opens the saved target on the connected workshop without a coding runtime override", async () => {
  api.targets.mockResolvedValue([{ target_id: "ssh-nas", name: "NAS", host: "nas.local", port: 22, username: "ops", agent_access: false }]);
  api.action.mockResolvedValue({ session_id: "ssh-session", error: null });
  component = mount(SettingsSshSection, { target: document.body }); await settle();
  button("Terminal").click(); await settle();
  expect(api.action).toHaveBeenCalledWith("terminal", expect.objectContaining({ target_id: "ssh-nas", request_key: expect.any(String) }));
  expect(api.terminal).toHaveBeenCalledWith("ssh-session", { title: "SSH · NAS", executionRuntimeId: null });
  expect(api.navigate).toHaveBeenCalledWith("chat");
});

it("keeps the saved access state visible when changing the grant fails", async () => {
  api.targets.mockResolvedValue([{ target_id: "ssh-nas", name: "NAS", host: "nas.local", port: 22, username: "ops", agent_access: true }]);
  api.access.mockRejectedValue(new Error("Workshop unavailable"));
  component = mount(SettingsSshSection, { target: document.body }); await settle();
  const grant = input("Agent access");
  grant.click(); await settle();
  expect(api.access).toHaveBeenCalledWith("ssh-nas", false);
  expect(grant.checked).toBe(true);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Workshop unavailable");
});
