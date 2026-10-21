/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const scope = vi.hoisted(() => ({ current: "a" }));
const api = vi.hoisted(() => ({ create: vi.fn(), history: vi.fn(), submit: vi.fn(), messages: vi.fn(() => []), bots: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => scope.current, workshopScopedStorageKey: (key: string, id: string) => `${key}:${id}` }));
vi.mock("$lib/daemon", () => ({ getSessionHistory: api.history, createSession: api.create, getActiveSessionTurn: async () => ({ active: false }), listAgentRuntimes: async () => ({ runtimes: [] }) }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { get workshopScopeId() { return scope.current; }, workshopEpoch: 1, sessionId: "unrelated", sessions: [{ session_id: "lab", display_name: "Labbing" }], messagesFor: api.messages, streamErrorFor: () => null, history: { open: vi.fn(), release: vi.fn() }, sessionRuntimes: new Map(), streamOwners: new Map(), warmBackgroundSession: vi.fn(), refreshSessions: vi.fn() } }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { bots: [{ bot_id: "taco", display_name: "Taco", primary_session_id: "taco-thread" }], refresh: api.bots, loading: false, error: null } }));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: { offline: false } }));
vi.mock("$lib/chat/submitTurnController", () => ({ submitChatTurn: api.submit }));
vi.mock("$lib/chat/prepareAgentSession", () => ({ prepareAgentSession: vi.fn() }));
vi.mock("$lib/chat/runtimeBotTurns.svelte", () => ({ refreshRuntimeBotTurn: vi.fn(), runtimeBotJob: () => null, runtimeBotJobPending: () => false }));
import Harness from "./test-fixtures/CompanionConversationHarness.svelte";
import type { CompanionConversation } from "./companionConversation.svelte";
import { loadAddressedDraft, saveAddressedDraft } from "./addressedDraft";
let component: ReturnType<typeof mount> | null = null;
let model: CompanionConversation;
async function setup(extra: Record<string, unknown> = {}) {
  component = mount(Harness, { target: document.body, props: { ...extra, expose: value => model = value } });
  flushSync();
  await vi.waitFor(() => expect(model.restoring).toBe(false));
  flushSync();
}
function type(text: string) { const input = document.querySelector("input")!; input.value = text; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); }
function enter() { document.querySelector("input")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); flushSync(); }
beforeEach(() => { scope.current = "a"; localStorage.clear(); vi.clearAllMocks(); api.create.mockResolvedValue({ session_id: "new-session" }); api.history.mockResolvedValue({ turns: [], authority_id: "" }); api.submit.mockImplementation(async ({ onAccepted }) => onAccepted?.({})); });
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); });
it("starts lazily from its one field and keeps follow-ups independent of focused Chat", async () => {
  await setup(); expect(document.querySelectorAll("input")).toHaveLength(1); expect(document.querySelector("textarea")).toBeNull();
  expect(api.create).not.toHaveBeenCalled(); type("Look this up"); enter();
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "new-session", prompt: "Look this up" })));
  flushSync(); type("Keep going"); enter();
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalledTimes(2)); expect(api.create).toHaveBeenCalledTimes(1);
  expect(loadAddressedDraft("a").address).toMatchObject({ sessionId: "new-session" });
});
it("continues saved sessions, survives renderer recreation, and restores bot bindings", async () => {
  await setup(); type("@labbing"); enter(); expect(model.conversation.sessionId).toBe("lab");
  type("Draft for Labbing"); await unmount(component!); component = null;
  await setup(); expect(model.query).toBe("Draft for Labbing"); expect(model.conversation.sessionId).toBe("lab");
  await unmount(component!); component = null;
  saveAddressedDraft("a", { query: "For Taco", address: { kind: "bot", label: "Taco", botId: "taco" } });
  await setup(); expect(model.conversation.sessionId).toBe("taco-thread");
});
it("keeps a missing bot draft until an explicit destination is chosen", async () => {
  saveAddressedDraft("a", { query: "For missing bot", address: { kind: "bot", label: "Gone", botId: "gone" } });
  await setup(); expect(document.body.textContent).toContain("Select a destination again"); enter(); expect(api.submit).not.toHaveBeenCalled();
  expect(loadAddressedDraft("a").address).toMatchObject({ botId: "gone" });
});
it("preserves failed delivery text and never submits it to a different workshop", async () => {
  await setup(); type("@labbing"); enter(); api.submit.mockRejectedValueOnce(new Error("Unavailable")); type("Only here"); enter();
  await vi.waitFor(() => expect(model.conversation.error).toBe("Unavailable")); expect(model.query).toBe("Only here");
  scope.current = "b"; await expect(model.conversation.send("Wrong place")).rejects.toThrow("Workshop changed"); expect(api.submit).toHaveBeenCalledTimes(1);
});

it("collapsing the floating bar leaves delivery and the addressed follow-up intact", async () => {
  await setup(); let admit!: () => void;
  api.submit.mockImplementationOnce(({ onAccepted }) => new Promise<void>(resolve => { admit = () => { onAccepted?.({}); resolve(); }; }));
  type("Keep working"); enter();
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalled());
  const harness = component as unknown as { setVisible: (value: boolean) => void };
  harness.setVisible(false); flushSync(); expect(document.querySelector("input")).toBeNull();
  admit(); await vi.waitFor(() => expect(model.conversation.sending).toBe(false));
  harness.setVisible(true); flushSync();
  expect(model.query).toBe(""); expect(model.conversation.sessionId).toBe("new-session"); expect(api.create).toHaveBeenCalledTimes(1);
});

it("opens results below the original field and searches tools without sending a chat turn", async () => {
  const run = vi.fn(); const body = vi.fn();
  await setup({ actions: [{ id: "note", label: "New note", keywords: "sticky", icon: "note", run }], onBodyChange: body });
  const original = document.querySelector("input");
  expect(body).toHaveBeenLastCalledWith(false);
  expect(document.querySelector('[aria-label="Companion results"]')).toBeNull();
  type("sticky"); expect(document.querySelector("input")).toBe(original);
  expect(body).toHaveBeenLastCalledWith(true);
  expect(document.querySelector('[aria-label="Companion results"]')?.textContent).toContain("New note");
  enter(); await vi.waitFor(() => expect(run).toHaveBeenCalledOnce()); expect(api.submit).not.toHaveBeenCalled();
  expect(document.querySelector("input")).toBe(original);
});
it("keeps @ destinations inside the body and switches to the transcript without replacing the field", async () => {
  await setup(); const original = document.querySelector("input");
  type("@labbing"); expect(document.querySelector(".command-spotlight-target-picker-inline")).not.toBeNull();
  enter(); expect(document.querySelector("input")).toBe(original);
  expect(document.querySelector('[aria-label="Recent conversation"]')).not.toBeNull();
  expect(document.querySelector(".command-spotlight-target-picker-inline")).toBeNull();
});
it("collapsing with an unsent draft keeps both its text and exact target", async () => {
  await setup(); type("@labbing"); enter(); type("Keep this unsent");
  const harness = component as unknown as { setVisible: (value: boolean) => void };
  harness.setVisible(false); flushSync(); harness.setVisible(true); flushSync();
  expect(model.query).toBe("Keep this unsent"); expect(model.conversation.sessionId).toBe("lab");
  expect(document.querySelector("input")?.value).toBe("Keep this unsent"); expect(api.submit).not.toHaveBeenCalled();
});
