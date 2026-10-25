/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const api = vi.hoisted(() => ({ history: vi.fn(), submit: vi.fn(), messages: vi.fn(() => []), native: false, label: "desktop-spotlight", focus: vi.fn(async () => {}), readImage: vi.fn(), drag: null as null | ((event: { payload: { type: string; position: { x: number; y: number }; paths: string[] } }) => void) }));
vi.mock("$lib/platform", async importOriginal => ({ ...await importOriginal<typeof import("$lib/platform")>(), isTauriDesktop: () => api.native }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: api.label, scaleFactor: async () => 2, setFocus: api.focus }) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: async (callback: typeof api.drag) => { api.drag = callback; return () => { api.drag = null; }; } }) }));
vi.mock("$lib/daemon", () => ({ getSessionHistory: api.history, readMediaImagePath: api.readImage, createSession: vi.fn(), getActiveSessionTurn: async () => ({ active: false }), listAgentRuntimes: async () => ({ runtimes: [] }) }));
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
afterEach(async () => { cleanup?.(); if (inputComponent) await unmount(inputComponent); if (transcript) await unmount(transcript); document.body.replaceChildren(); vi.clearAllMocks(); api.native = false; api.label = "desktop-spotlight"; api.drag = null; });
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
it("uses AppKit arrow keys to navigate @ destinations", async () => {
  const conversation = await setup("@");
  key("\uF701"); key("Enter");
  expect(conversation.sessionId).toBe("lab");
  expect(input().value).toBe("");
});
it("dismisses the picker before allowing Escape to close Spotlight", async () => {
  await setup("@"); const first = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }); input().dispatchEvent(first); flushSync();
  expect(first.defaultPrevented).toBe(true); expect(document.querySelector('[role="listbox"]')).toBeNull();
  const next = new KeyboardEvent("keydown", { key: "Escape", cancelable: true }); expect(inputComponent.handleKeydown(next)).toBe(false);
});
it("keeps a failed message in the same input and renders the delivery error in the body", async () => {
  await setup("@labbing"); key("Enter"); api.submit.mockRejectedValueOnce(new Error("Workshop unreachable")); type("Do this"); key("Enter");
  await vi.waitFor(() => expect(document.body.textContent).toContain("Workshop unreachable")); expect(input().value).toBe("Do this");
  const failure = document.querySelector<HTMLDetailsElement>(".command-spotlight-conversation-failure")!;
  expect(failure.open).toBe(false);
  expect(failure.querySelector("summary")?.textContent).toBe("Couldn’t send this message");
  expect(document.querySelector('[role="alert"]')).toBeNull();
});

it("renders the saved Markdown answer and clears Working after a missed completion", async () => {
  const conversation = await setup("@labbing"); key("Enter");
  api.messages.mockReturnValue([{ id: "pending", role: "assistant", turnId: "done-turn", content: "", streaming: true, statusLine: "Running cognition_store_write" }] as never[]);
  api.history.mockResolvedValue({ authority_id: "", turns: [{ role: "assistant", content: "Your **design** is ready.\n\n- Editable text\n- Shapes", caused_by: { execution_id: "done-turn" }, timestamp: "2026-10-25T02:03:00Z" }] });
  await conversation.refresh(); flushSync();
  expect(document.body.querySelector("strong")?.textContent).toBe("design");
  expect(document.body.querySelectorAll(".markdown-content li")).toHaveLength(2);
  expect(document.body.textContent).not.toContain("Working");
  expect(document.body.textContent).not.toContain("cognition_store_write");
  expect(input().placeholder).toBe("Message Medousa…");
  expect(document.querySelector(".command-spotlight-recipient")?.textContent).toContain("@Medousa");
});

it("shows a quiet working indicator instead of a fake assistant reply while waiting", async () => {
  await setup("@labbing");
  api.messages.mockReturnValue([{ id: "pending", role: "assistant", content: "", streaming: true, statusLine: "interactive turn accepted; agent runtime started" }] as never[]);
  key("Enter");
  expect(document.body.textContent).toContain("Working…");
  expect(document.body.textContent).not.toContain("agent runtime");
  expect(document.querySelector(".command-spotlight-transcript-speaker")).toBeNull();
});

it("attaches a native image drop and focuses the composer without changing native window focus", async () => {
  api.native = true;
  api.readImage.mockResolvedValue({ bytes: new Uint8Array([137, 80, 78, 71]), filename: "screenshot.png", mime: "image/png" });
  const conversation = await setup("");
  const area = document.querySelector('[aria-label="Spotlight message; paste or drop screenshots"]')!;
  vi.spyOn(area, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 800, 500));
  await vi.waitFor(() => expect(api.drag).not.toBeNull());
  api.drag!({ payload: { type: "enter", position: { x: 100, y: 100 }, paths: [] } });
  expect(api.focus).not.toHaveBeenCalled();
  api.drag!({ payload: { type: "drop", position: { x: 100, y: 100 }, paths: ["/client/screenshot.png"] } });
  await vi.waitFor(() => expect(conversation.files).toHaveLength(1));
  expect(api.readImage).toHaveBeenCalledWith("/client/screenshot.png");
  expect(api.focus).not.toHaveBeenCalled();
  await new Promise(resolve => requestAnimationFrame(resolve));
  expect(document.activeElement).toBe(input());
});
it("does not steal focus when a drop finishes reading after Spotlight was dismissed", async () => {
  api.native = true;
  let resolve!: (value: { bytes: Uint8Array; filename: string; mime: string }) => void;
  api.readImage.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  const conversation = await setup("");
  const area = document.querySelector('[aria-label="Spotlight message; paste or drop screenshots"]')!;
  vi.spyOn(area, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 800, 500));
  await vi.waitFor(() => expect(api.drag).not.toBeNull());
  api.drag!({ payload: { type: "drop", position: { x: 100, y: 100 }, paths: ["/client/screenshot.png"] } });
  await vi.waitFor(() => expect(api.readImage).toHaveBeenCalled());
  expect(api.focus).not.toHaveBeenCalled();
  await unmount(inputComponent); inputComponent = undefined as never;
  resolve({ bytes: new Uint8Array([137, 80, 78, 71]), filename: "screenshot.png", mime: "image/png" });
  await new Promise(done => setTimeout(done, 0));
  expect(conversation.files).toHaveLength(0);
  expect(api.focus).not.toHaveBeenCalled();
});

it("keeps native file drops working in the in-app palette without launcher-only focus permission", async () => {
  api.native = true; api.label = "main";
  api.readImage.mockResolvedValue({ bytes: new Uint8Array([137, 80, 78, 71]), filename: "screenshot.png", mime: "image/png" });
  const conversation = await setup("");
  const area = document.querySelector('[aria-label="Spotlight message; paste or drop screenshots"]')!;
  vi.spyOn(area, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 800, 500));
  await vi.waitFor(() => expect(api.drag).not.toBeNull());
  api.drag!({ payload: { type: "drop", position: { x: 100, y: 100 }, paths: ["/client/screenshot.png"] } });
  await vi.waitFor(() => expect(conversation.files).toHaveLength(1));
  expect(api.focus).not.toHaveBeenCalled();
});
