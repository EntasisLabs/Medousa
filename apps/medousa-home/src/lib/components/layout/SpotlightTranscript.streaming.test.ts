/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import type { ChatMessage } from "$lib/types/chat";
import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";

const api = vi.hoisted(() => ({ read: vi.fn(), imageUrl: vi.fn(), revoke: vi.fn() }));
vi.mock("$lib/daemon", () => ({ readMediaBytes: api.read, createVaultNote: vi.fn() }));
vi.mock("$lib/utils/chatImagePreview", () => ({ chatImageObjectUrl: api.imageUrl }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { pendingMediaRefs: [], draft: "" } }));
// Happy DOM's parser drops the outer wrapper during DOMPurify sanitization.
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));

import SpotlightTranscript from "./SpotlightTranscript.svelte";
import ChatMediaParts from "$lib/components/chat/ChatMediaParts.svelte";

let component: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  vi.clearAllMocks();
  api.read.mockResolvedValue({ bytes: new Uint8Array([137, 80, 78, 71]), mime: "image/png" });
  api.imageUrl.mockImplementation(async () => `blob:preview-${api.imageUrl.mock.calls.length}`);
  vi.stubGlobal("URL", class extends URL {
    static revokeObjectURL = api.revoke;
  });
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

const initial: ChatMessage[] = [
  {
    id: "user", role: "user", content: "Wireframe this",
    mediaAttachments: [{ mediaId: "screenshot", kind: "image", mime: "image/png", label: "Screenshot.png" }],
  },
  {
    id: "reply", role: "assistant", streaming: true,
    content: "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n\nWorking",
  },
];

it("retains the attached image and completed Markdown blocks across streamed updates and refreshed message objects", async () => {
  const store = writable({ session: "spotlight-session", messages: structuredClone(initial) });
  const state = fromStore(store);
  const conversation = {
    get messages() { return state.current.messages.map(message => ({ ...message })); },
    get sessionId() { return state.current.session; },
    get working() { return state.current.messages.some(message => message.streaming); },
    assistantLabel: "Medousa", sending: false, loading: false, error: null, failure: null,
  } as AddressedConversation;
  component = mount(SpotlightTranscript, { target: document.body, props: { conversation, onOpen: () => {} } });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector(".chat-media-thumbnail-image")).not.toBeNull());
  const thumbnail = document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")!;
  const src = thumbnail.src;
  const paragraph = document.querySelector(".markdown-content p")!;
  document.querySelector<HTMLButtonElement>(".chat-media-thumbnail")!.click();
  flushSync();
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  for (const suffix of [" on", " the", " editable", " design."]) {
    store.update(value => ({
      ...value,
      messages: value.messages.map(message => ({
        ...message,
        content: message.role === "assistant" ? message.content + suffix : message.content,
        mediaAttachments: message.mediaAttachments?.map(attachment => ({ ...attachment })),
      })),
    }));
    await tick();
    expect(document.querySelector(".chat-media-thumbnail-image")).toBe(thumbnail);
    expect(thumbnail.src).toBe(src);
    expect(document.querySelector(".markdown-content p")).toBe(paragraph);
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  }
  store.update(value => ({
    ...value,
    messages: value.messages.map(message => ({ ...message, streaming: false })),
  }));
  await tick();
  expect(document.querySelector(".chat-media-thumbnail-image")).toBe(thumbnail);
  expect(document.querySelector(".markdown-content p")).toBe(paragraph);
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  expect(api.read).toHaveBeenCalledExactlyOnceWith("spotlight-session", "screenshot");
  expect(api.imageUrl).toHaveBeenCalledTimes(1);
  expect(api.revoke).not.toHaveBeenCalled();
});

it("reloads the image for a different session and releases each session's object URL", async () => {
  const store = writable("first-session");
  const state = fromStore(store);
  component = mount(ChatMediaParts, { target: document.body, props: {
    get sessionId() { return state.current; }, attachments: structuredClone(initial[0].mediaAttachments!),
  } });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector(".chat-media-thumbnail-image")).not.toBeNull());
  const first = document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")!.src;
  store.set("second-session");
  await tick();
  await vi.waitFor(() => expect(api.read).toHaveBeenLastCalledWith("second-session", "screenshot"));
  await vi.waitFor(() => expect(document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")?.src).toBe("blob:preview-2"));
  expect(api.revoke).toHaveBeenCalledWith(first);
  const second = document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")!.src;
  await unmount(component!);
  component = undefined;
  expect(api.revoke).toHaveBeenCalledWith(second);
});

it("discards a late image read after changing sessions without disturbing the new thumbnail", async () => {
  let finish!: (value: { bytes: Uint8Array; mime: string }) => void;
  api.read.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const store = writable("first-session");
  const state = fromStore(store);
  component = mount(ChatMediaParts, { target: document.body, props: {
    get sessionId() { return state.current; }, attachments: structuredClone(initial[0].mediaAttachments!),
  } });
  flushSync();
  expect(api.read).toHaveBeenCalledWith("first-session", "screenshot");
  store.set("second-session");
  await tick();
  await vi.waitFor(() => expect(document.querySelector(".chat-media-thumbnail-image")).not.toBeNull());
  const thumbnail = document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")!;
  finish({ bytes: new Uint8Array([137, 80, 78, 71]), mime: "image/png" });
  await vi.waitFor(() => expect(api.imageUrl).toHaveBeenCalledTimes(2));
  await tick();
  expect(document.querySelector(".chat-media-thumbnail-image")).toBe(thumbnail);
  expect(thumbnail.src).toBe("blob:preview-1");
  expect(api.revoke).toHaveBeenCalledWith("blob:preview-2");
  expect(api.revoke).not.toHaveBeenCalledWith("blob:preview-1");
});

it("loads a changed media source without reusing the previous image", async () => {
  const store = writable(structuredClone(initial[0].mediaAttachments!));
  const state = fromStore(store);
  component = mount(ChatMediaParts, { target: document.body, props: {
    sessionId: "spotlight-session",
    get attachments() { return state.current; },
  } });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector(".chat-media-thumbnail-image")).not.toBeNull());
  store.update(attachments => attachments.map(attachment => ({ ...attachment, mediaId: "replacement" })));
  await tick();
  await vi.waitFor(() => expect(document.querySelector<HTMLImageElement>(".chat-media-thumbnail-image")?.src).toBe("blob:preview-2"));
  expect(api.read).toHaveBeenLastCalledWith("spotlight-session", "replacement");
  expect(api.revoke).toHaveBeenCalledWith("blob:preview-1");
});
