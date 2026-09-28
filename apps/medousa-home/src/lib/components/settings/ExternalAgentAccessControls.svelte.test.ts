/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { ExternalConversation } from "$lib/daemon/externalConversations";

const api = vi.hoisted(() => ({ createExternalAgentToken: vi.fn(), revokeExternalAgentToken: vi.fn() }));
vi.mock("$lib/daemon/externalConversations", () => api);
import ExternalAgentAccessControls from "./ExternalAgentAccessControls.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
  vi.resetAllMocks();
});

function setup(access?: ExternalConversation["api_access"]) {
  const state = $state<{ conversation: ExternalConversation }>({
    conversation: { id: "instinct-test", provider: "instinct", label: "Instinct", target: "15551234567@s.whatsapp.net", created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z", events: [], api_access: access },
  });
  const onchange = vi.fn((value: ExternalConversation) => { state.conversation = value; });
  const target = document.createElement("div");
  document.body.append(target);
  component = mount(ExternalAgentAccessControls, { target, props: { get conversation() { return state.conversation; }, onchange } });
  flushSync();
  return { state, onchange };
}

function button(label: string) {
  const result = [...document.querySelectorAll("button")].find((item) => item.textContent?.trim() === label);
  expect(result, label).toBeTruthy();
  return result!;
}

const grant = { scopes: ["read", "work"] as ("read" | "work")[], expires_at: "2026-10-27T00:00:00Z" };

it("issues selected permissions and expiry, then hides the token on Done", async () => {
  const { onchange } = setup();
  api.createExternalAgentToken.mockResolvedValue({ token: "synthetic-once-token", access: grant });
  const work = document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1];
  work.click();
  const days = document.querySelector<HTMLInputElement>('input[type="number"]')!;
  days.value = "7";
  days.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Create API token").click();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector("code")?.textContent).toBe("synthetic-once-token"); });
  expect(api.createExternalAgentToken).toHaveBeenCalledWith("instinct-test", { scopes: ["read", "work"], expires_in_days: 7 });
  expect(onchange).toHaveBeenCalledWith(expect.objectContaining({ api_access: grant }));
  button("Done").click();
  flushSync();
  expect(document.querySelector("code")).toBeNull();
  expect(button("Replace API token")).toBeTruthy();
});

it("preserves existing scopes on replacement and revokes access", async () => {
  const { state } = setup({ scopes: ["work"], expires_at: grant.expires_at });
  const checkboxes = document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
  expect(checkboxes[0].checked).toBe(false);
  expect(checkboxes[1].checked).toBe(true);
  api.createExternalAgentToken.mockResolvedValue({ token: "replacement-token", access: state.conversation.api_access });
  button("Replace API token").click();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector("code")).toBeTruthy(); });
  expect(api.createExternalAgentToken).toHaveBeenCalledWith("instinct-test", { scopes: ["work"], expires_in_days: 30 });
  api.revokeExternalAgentToken.mockResolvedValue({ ...state.conversation, api_access: undefined });
  button("Revoke API token").click();
  await vi.waitFor(() => { flushSync(); expect(state.conversation.api_access).toBeUndefined(); });
  expect(api.revokeExternalAgentToken).toHaveBeenCalledWith("instinct-test");
  expect(document.querySelector("code")).toBeNull();
  expect(button("Create API token")).toBeTruthy();
});

it("keeps the existing grant and exposes an error when replacement fails", async () => {
  const { onchange } = setup(grant);
  api.createExternalAgentToken.mockRejectedValue(new Error("Workshop unavailable"));
  button("Replace API token").click();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector('[role="alert"]')?.textContent).toBe("Workshop unavailable"); });
  expect(onchange).not.toHaveBeenCalled();
  expect(document.querySelector("code")).toBeNull();
  expect(button("Replace API token").disabled).toBe(false);
});
