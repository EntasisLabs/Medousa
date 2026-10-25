/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, tick, unmount } from "svelte";
import type { PendingAgentPermission } from "$lib/types/chat";
const api = vi.hoisted(() => ({ approve: vi.fn(), deny: vi.fn(), resolved: vi.fn(), pending: null as PendingAgentPermission | null }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { get permissionAlert() { return api.pending; }, notePermissionResolved: api.resolved } }));
vi.mock("$lib/daemon", () => ({ approveAgentPermission: api.approve, denyAgentPermission: api.deny }));
vi.mock("$lib/platform", () => ({ homeChannelSurface: () => "home" }));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
import AgentPermissionBar from "./AgentPermissionBar.svelte";
let component: ReturnType<typeof mount>;
beforeEach(() => { vi.clearAllMocks(); api.pending = { requestId: "request", turnId: "turn", messageId: null, agentSessionId: "agent", agentRuntime: "pi", message: "Choose" }; });
afterEach(async () => { if (component) await unmount(component); document.body.innerHTML = ""; });
const button = (name: string) => [...document.querySelectorAll("button")].find(item => item.textContent?.trim() === name)!;
it("forwards an exact provider choice including rejection through the selected-option route", async () => {
  api.pending!.options = [{ option_id: "allow-all", name: "Always allow", kind: "allow_always" }, { option_id: "reject-once", name: "No thanks", kind: "reject_once" }];
  component = mount(AgentPermissionBar, { target: document.body }); await tick();
  expect(button("Allow")).toBeUndefined();
  button("No thanks").click(); await tick(); await tick();
  expect(api.approve).toHaveBeenCalledWith("request", "home", "reject-once");
  expect(api.deny).not.toHaveBeenCalled();
});
it("preserves Allow and Deny for old daemon permission messages", async () => {
  component = mount(AgentPermissionBar, { target: document.body }); await tick();
  button("Allow").click(); await tick(); await tick();
  expect(api.approve).toHaveBeenCalledWith("request", "home", undefined);
  expect(button("Deny")).toBeDefined();
});
