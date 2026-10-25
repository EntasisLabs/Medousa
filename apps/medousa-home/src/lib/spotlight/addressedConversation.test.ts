import { beforeEach, expect, it, vi } from "vitest";
import type { ConversationTarget } from "./conversationTargets";
const state = vi.hoisted(() => ({ scope: "workshop-a", epoch: 1 }));
const api = vi.hoisted(() => ({ upload: vi.fn(), create: vi.fn(), history: vi.fn(), active: vi.fn(), submit: vi.fn(), open: vi.fn(), warm: vi.fn(), release: vi.fn(), messages: vi.fn(() => []) }));
vi.mock("$lib/utils/chatMediaUpload", () => ({ uploadChatFiles: api.upload }));
vi.mock("$lib/daemon", () => ({ createSession: api.create, getSessionHistory: api.history, getActiveSessionTurn: api.active, listAgentRuntimes: async () => ({ runtimes: [] }) }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: {
  get workshopScopeId() { return state.scope; }, get workshopEpoch() { return state.epoch; }, sessionId: "unrelated-chat",
  messagesFor: api.messages, streamErrorFor: () => null, warmBackgroundSession: api.warm, history: { open: vi.fn(), release: api.release },
  sessionRuntimes: new Map(), streamOwners: new Map(), refreshSessions: vi.fn(),
} }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { open: api.open } }));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: { offline: false } }));
vi.mock("$lib/chat/submitTurnController", () => ({ submitChatTurn: api.submit }));
vi.mock("$lib/chat/prepareAgentSession", () => ({ prepareAgentSession: vi.fn() }));
vi.mock("$lib/chat/runtimeBotTurns.svelte", () => ({ refreshRuntimeBotTurn: vi.fn(), runtimeBotJob: () => null, runtimeBotJobPending: () => false }));
import { AddressedConversation, transcriptExcerpt } from "./addressedConversation.svelte";
import { chat } from "$lib/stores/chat.svelte";
const session: ConversationTarget = { id: "session:labbing", kind: "session", sessionId: "labbing", label: "Labbing", group: "Conversations", description: "", scope: "workshop-a" };
const fresh: ConversationTarget = { id: "new:medousa", kind: "new", runtime: "medousa", label: "Medousa", group: "Start new", description: "", scope: "workshop-a" };
beforeEach(() => {
  vi.clearAllMocks(); api.messages.mockReturnValue([]); state.scope = "workshop-a"; state.epoch = 1;
  api.create.mockResolvedValue({ session_id: "created" }); api.history.mockResolvedValue({ authority_id: "", session_id: "labbing", turns: [] });
  api.active.mockResolvedValue({ active: false }); api.submit.mockResolvedValue(undefined); api.warm.mockResolvedValue(undefined);
});
it("previews only the addressed session and sends without changing the focused conversation", async () => {
  const controller = new AddressedConversation(); controller.select(session); await controller.refresh(); await controller.send("Continue here");
  expect(api.history).toHaveBeenCalledWith("labbing", { limit: 6 });
  expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "labbing", prompt: "Continue here" }));
  expect(chat.sessionId).toBe("unrelated-chat"); expect(api.create).not.toHaveBeenCalled();
});
it("creates only on first send, then follows up in the same new session", async () => {
  const controller = new AddressedConversation(); controller.select(fresh); expect(api.create).not.toHaveBeenCalled();
  await controller.send("Look this up"); await controller.send("One more thing");
  expect(api.create).toHaveBeenCalledTimes(1);
  expect(api.submit).toHaveBeenLastCalledWith(expect.objectContaining({ sessionId: "created", prompt: "One more thing" }));
});
it("keeps the created destination after a failed admission so retry does not create another session", async () => {
  const controller = new AddressedConversation(); controller.select(fresh); api.submit.mockRejectedValueOnce(new Error("Try again"));
  await expect(controller.send("Hello")).rejects.toThrow("Try again"); expect(controller.error).toBe("Try again");
  await controller.send("Hello"); expect(api.create).toHaveBeenCalledTimes(1);
});
it("opens a bot's own binding instead of the currently selected chat", async () => {
  const controller = new AddressedConversation(); api.open.mockResolvedValue({ binding: { session_id: "taco-session" } });
  controller.select({ id: "bot:taco", kind: "bot", bot: { bot_id: "taco", display_name: "Taco" } as never, label: "Taco", group: "Bots", scope: "workshop-a", description: "" });
  await controller.send("Handle this");
  expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "taco-session" }));
});
it("does not admit to a different workshop after an asynchronous lookup", async () => {
  const controller = new AddressedConversation(); controller.select(fresh);
  api.create.mockImplementationOnce(async () => { state.epoch += 1; state.scope = "other"; return { session_id: "old" }; });
  await expect(controller.send("Hello")).rejects.toThrow("Workshop changed"); expect(api.submit).not.toHaveBeenCalled();
});
it("ignores a stale transcript when another target is selected", async () => {
  let finish!: (value: unknown) => void; api.history.mockReturnValueOnce(new Promise(resolve => finish = resolve));
  const controller = new AddressedConversation(); controller.select(session); controller.select(fresh);
  finish({ authority_id: "", turns: [{ role: "assistant", content: "Old conversation", timestamp: "today" }] });
  await Promise.resolve(); await Promise.resolve(); expect(controller.messages).toEqual([]);
});
it("dismissal neither cancels nor redirects a send already in progress", async () => {
  let finish!: () => void; api.submit.mockImplementationOnce(() => new Promise<void>(resolve => finish = resolve));
  const controller = new AddressedConversation(); controller.select(session); const sending = controller.send("Keep working");
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalled()); controller.clear(); finish(); await sending;
  expect(api.submit).toHaveBeenCalledTimes(1); expect(controller.target).toBeNull();
});
it("rejects double submission and uses background admission when the conversation is already running", async () => {
  let finish!: () => void; api.submit.mockImplementationOnce(() => new Promise<void>(resolve => finish = resolve)); api.active.mockResolvedValue({ active: true });
  const controller = new AddressedConversation(); controller.select(session); const sending = controller.send("First");
  await expect(controller.send("Second")).rejects.toThrow("still sending");
  await vi.waitFor(() => expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ mode: "background" }))); finish(); await sending;
});
it("bounds a transcript to six short entries without mutating the source", () => {
  const source = Array.from({ length: 20 }, (_, id) => ({ id: String(id), role: "assistant" as const, content: "x".repeat(10000) }));
  const excerpt = transcriptExcerpt(source); expect(excerpt).toHaveLength(6); expect(excerpt.every(message => message.content.length <= 4000)).toBe(true); expect(source[0].content).toHaveLength(10000);
});

it("keeps a just-finished live reply visible when its assistant entry has no timestamp", async () => {
  const controller = new AddressedConversation(); controller.select(session);
  api.history.mockResolvedValue({ authority_id: "", turns: [{ role: "assistant", content: "Older reply", timestamp: "2026-10-19T00:00:00Z" }] });
  await controller.refresh();
  api.messages.mockReturnValue([{ id: "user", role: "user", content: "Latest question", createdAt: "2026-10-20T00:00:00Z" }, { id: "reply", role: "assistant", content: "Latest answer", streaming: false }] as never[]);
  expect(controller.messages.at(-1)?.content).toBe("Latest answer");
});

it("labels the assistant with its runtime instead of repeating a long conversation title", () => {
  const controller = new AddressedConversation(); controller.select({ ...session, label: "A long conversation title" });
  expect(controller.assistantLabel).toBe("Medousa");
});

it("keeps startup chatter out of Spotlight excerpts and preserves actual user text", () => {
  const noise = "interactive turn accepted; agent runtime started";
  const messages = transcriptExcerpt([{ id: "user", role: "user", content: noise }, { id: "pending", role: "assistant", content: "", streaming: true, statusLine: noise }, { id: "old", role: "assistant", content: noise }]);
  expect(messages).toHaveLength(2); expect(messages[0].content).toBe(noise);
  expect(messages[1]).toMatchObject({ content: "", streaming: true, statusLine: null });
});

it("keeps screenshot bytes local until the addressed session exists, and reuses uploaded media on retry", async () => {
  const controller = new AddressedConversation();
  const file = new File(["pixels"], "Screenshot.png", { type: "image/png" });
  controller.addFiles([file]); controller.select(fresh);
  const media = [{ media_id: "image_1", kind: "image", mime: "image/png" }];
  api.upload.mockResolvedValue(media); api.submit.mockRejectedValueOnce(new Error("temporary failure"));
  controller.references = [{ kind: "vault_note", root_id: "obsidian", path: "spec.md", title: "Spec" }];
  expect(api.upload).not.toHaveBeenCalled();
  await expect(controller.send("Make a wireframe")).rejects.toThrow("temporary failure");
  expect(controller.files).toHaveLength(1);
  await controller.send("Make a wireframe");
  expect(api.create).toHaveBeenCalledTimes(1); expect(api.upload).toHaveBeenCalledTimes(1);
  expect(api.submit).toHaveBeenLastCalledWith(expect.objectContaining({ sessionId: "created", prompt: "Make a wireframe", userContent: "Make a wireframe", mediaRefs: media,
    hostContext: expect.objectContaining({ references: [expect.objectContaining({ root_id: "obsidian" })] }) }));
});
it("does not upload or admit into a new workshop after the upload workshop changes", async () => {
  const controller = new AddressedConversation(); controller.select(fresh);
  controller.addFiles([new File(["pixels"], "Screenshot.png", { type: "image/png" })]);
  api.upload.mockImplementationOnce(async () => { state.epoch++; state.scope = "workshop-b"; return []; });
  await expect(controller.send("Wireframe this")).rejects.toThrow("conversation changed");
  expect(api.submit).not.toHaveBeenCalled();
});
