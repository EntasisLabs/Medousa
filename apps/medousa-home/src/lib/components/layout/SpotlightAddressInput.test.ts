/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const api = vi.hoisted(() => ({ history: vi.fn(), submit: vi.fn(), messages: vi.fn(() => []) }));
vi.mock("$lib/daemon", () => ({ getSessionHistory: api.history, createSession: vi.fn(), getActiveSessionTurn: async () => ({ active: false }), listAgentRuntimes: async () => ({ runtimes: [] }) }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { workshopScopeId: "test", workshopEpoch: 1, sessionId: "other", sessions: [{ session_id: "lab", display_name: "Labbing", preview: "Recent work", turns: 2, verification_runs: 0 }], messagesFor: api.messages, streamErrorFor: () => null, history: { open: vi.fn() }, sessionRuntimes: new Map(), warmBackgroundSession: vi.fn(), refreshSessions: vi.fn() } }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { bots: [{ bot_id: "taco", display_name: "Taco", primary_session_id: "taco-thread" }], loading: false, error: null } }));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: { offline: false } }));
vi.mock("$lib/chat/submitTurnController", () => ({ submitChatTurn: api.submit }));
vi.mock("$lib/chat/prepareAgentSession", () => ({ prepareAgentSession: vi.fn() }));
vi.mock("$lib/chat/runtimeBotTurns.svelte", () => ({ refreshRuntimeBotTurn: vi.fn(), runtimeBotJob: () => null, runtimeBotJobPending: () => false }));
import SpotlightAddressInput from "./SpotlightAddressInput.svelte";
import SpotlightTranscript from "./SpotlightTranscript.svelte";
import { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
let inputComponent: { handleKeydown: (event: KeyboardEvent) => boolean };
let transcript: ReturnType<typeof mount>;
let cleanup: (() => void) | undefined;
async function setup(query: string) {
  api.messages.mockReturnValue([]);
  api.history.mockResolvedValue({ turns: [], authority_id: "" });
  api.submit.mockImplementation(async ({ onAccepted }) => { onAccepted?.({}); });
  const conversation = new AddressedConversation();
  inputComponent = mount(SpotlightAddressInput, { target: document.body, props: { query, inputEl: null, conversation, placeholder: "Search", busy: false, enabled: true, onQueryChange: () => {} } });
  transcript = mount(SpotlightTranscript, { target: document.body, props: { conversation, onOpen: () => {} } });
  const keydown = (event: KeyboardEvent) => inputComponent.handleKeydown(event);
  window.addEventListener("keydown", keydown); cleanup = () => window.removeEventListener("keydown", keydown);
  flushSync(); return conversation;
}
const input = () => document.querySelector<HTMLInputElement>("input")!;
function type(value: string) { input().value = value; input().dispatchEvent(new Event("input", { bubbles: true })); flushSync(); }
function key(value: string, options: KeyboardEventInit = {}) { input().dispatchEvent(new KeyboardEvent("keydown", { key: value, bubbles: true, cancelable: true, ...options })); flushSync(); }
afterEach(async () => { cleanup?.(); if (inputComponent) await unmount(inputComponent); if (transcript) await unmount(transcript); document.body.replaceChildren(); vi.clearAllMocks(); });
it("shows grouped destinations in an anchored picker, then uses the same field for messages", async () => {
  const conversation = await setup("@");
  expect(document.querySelectorAll("input")).toHaveLength(1); expect(document.body.textContent).toContain("Bots"); expect(document.body.textContent).toContain("Conversations"); expect(document.body.textContent).toContain("Start new");
  key("Enter"); expect(conversation.target?.label).toBe("Taco"); expect(input().value).toBe(""); expect(input().placeholder).toBe("Message Taco…"); expect(document.querySelector('[role="listbox"]')).toBeNull();
  type("Handle this"); key("Enter");
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "taco-thread", prompt: "Handle this" })));
  flushSync(); expect(input().value).toBe(""); expect(conversation.target?.label).toBe("Taco");
  key("Backspace"); expect(conversation.target).toBeNull(); expect(document.querySelectorAll("input")).toHaveLength(1);
});
it("filters targets, preserves a typed message on selection, and ignores IME Enter", async () => {
  const conversation = await setup("@labbing keep going"); key("Enter", { isComposing: true }); expect(conversation.target).toBeNull();
  key("Enter"); expect(conversation.sessionId).toBe("lab"); expect(input().value).toBe("keep going");
});
it("dismisses the picker before allowing Escape to close Spotlight", async () => {
  await setup("@"); const first = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }); input().dispatchEvent(first); flushSync();
  expect(first.defaultPrevented).toBe(true); expect(document.querySelector('[role="listbox"]')).toBeNull();
  const next = new KeyboardEvent("keydown", { key: "Escape", cancelable: true }); expect(inputComponent.handleKeydown(next)).toBe(false);
});
it("keeps a failed message in the same input and renders the delivery error in the body", async () => {
  await setup("@labbing"); key("Enter"); api.submit.mockRejectedValueOnce(new Error("Workshop unreachable")); type("Do this"); key("Enter");
  await vi.waitFor(() => expect(document.body.textContent).toContain("Workshop unreachable")); expect(input().value).toBe("Do this");
});

it("shows a quiet working indicator instead of a fake assistant reply while waiting", async () => {
  await setup("@labbing");
  api.messages.mockReturnValue([{ id: "pending", role: "assistant", content: "", streaming: true, statusLine: "interactive turn accepted; agent runtime started" }] as never[]);
  key("Enter");
  expect(document.body.textContent).toContain("Working…");
  expect(document.body.textContent).not.toContain("agent runtime");
  expect(document.querySelector(".command-spotlight-transcript-speaker")).toBeNull();
});
