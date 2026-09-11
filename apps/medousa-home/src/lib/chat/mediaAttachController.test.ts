// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatStoreHost } from "./chatStoreHost";
import type { MediaRef } from "$lib/types/media";
const mocks = vi.hoisted(() => ({
  readClipboardImages: vi.fn(), uploadChatFiles: vi.fn(),
  uploadChatPaths: vi.fn(), pickChatAttachmentFiles: vi.fn(),
}));
vi.mock("$lib/utils/chatImagePaste", () => ({ readClipboardImages: mocks.readClipboardImages }));
vi.mock("$lib/utils/chatMediaUpload", () => mocks);
import { attachClipboardImages, attachDroppedFiles, clearPendingMedia } from "./mediaAttachController";
import { MAX_MEDIA_REFS_PER_TURN } from "$lib/utils/normieErrors";
const ref: MediaRef = { media_id: "image", mime: "image/png", kind: "image" };
function host() {
  return { sessionId: "chat-a", workshopEpoch: 1, pendingMediaRefs: [],
    pendingMediaUploading: false, streamError: null, setError: vi.fn(),
  } as unknown as ChatStoreHost;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
beforeEach(() => {
  vi.resetAllMocks();
  mocks.readClipboardImages.mockResolvedValue([new File(["png"], "image.png", { type: "image/png" })]);
  mocks.uploadChatFiles.mockResolvedValue([ref]);
});
describe("composer attachment admission", () => {
  it("starts the clipboard read in the gesture and marks it busy through upload", async () => {
    const chat = host();
    const upload = deferred<MediaRef[]>();
    mocks.uploadChatFiles.mockReturnValue(upload.promise);
    const pending = attachClipboardImages(chat);
    expect(mocks.readClipboardImages).toHaveBeenCalledOnce();
    expect(chat.pendingMediaUploading).toBe(true);
    await Promise.resolve();
    expect(chat.pendingMediaUploading).toBe(true);
    upload.resolve([ref]);
    await pending;
    expect(chat.pendingMediaRefs).toEqual([ref]);
    expect(chat.pendingMediaUploading).toBe(false);
  });
  it.each(["session", "workshop"])("does not upload after changing %s during clipboard permission prompt", async (scope) => {
    const chat = host();
    const read = deferred<File[]>();
    mocks.readClipboardImages.mockReturnValue(read.promise);
    const pending = attachClipboardImages(chat);
    if (scope === "session") chat.sessionId = "chat-b";
    else { chat.workshopEpoch++; chat.pendingMediaUploading = false; }
    read.resolve([new File(["png"], "image.png")]);
    await pending;
    expect(mocks.uploadChatFiles).not.toHaveBeenCalled();
    expect(chat.pendingMediaRefs).toEqual([]);
    expect(chat.pendingMediaUploading).toBe(false);
  });
  it("ignores an in-flight upload result after switching chats", async () => {
    const chat = host();
    const upload = deferred<MediaRef[]>();
    mocks.uploadChatFiles.mockReturnValue(upload.promise);
    const pending = attachDroppedFiles(chat, [new File(["png"], "image.png")]);
    chat.sessionId = "chat-b";
    upload.resolve([ref]);
    await pending;
    expect(chat.pendingMediaRefs).toEqual([]);
  });
  it("clearing attachments invalidates the upload without clearing a newer operation's busy flag", async () => {
    const chat = host();
    const old = deferred<MediaRef[]>();
    const next = deferred<MediaRef[]>();
    mocks.uploadChatFiles.mockReturnValueOnce(old.promise).mockReturnValueOnce(next.promise);
    const file = new File(["png"], "image.png");
    const first = attachDroppedFiles(chat, [file]);
    clearPendingMedia(chat);
    const second = attachDroppedFiles(chat, [file]);
    old.resolve([ref]);
    await first;
    expect(chat.pendingMediaRefs).toEqual([]);
    expect(chat.pendingMediaUploading).toBe(true);
    next.resolve([ref]);
    await second;
    expect(chat.pendingMediaRefs).toEqual([ref]);
  });
  it("reports upload and clipboard errors and releases the busy state", async () => {
    const chat = host();
    mocks.readClipboardImages.mockRejectedValue(new Error("No image on clipboard"));
    await attachClipboardImages(chat);
    expect(chat.setError).toHaveBeenCalledWith("No image on clipboard");
    expect(chat.pendingMediaUploading).toBe(false);
  });
  it("does not read the clipboard when busy or full, and rejects oversized batches before upload", async () => {
    const chat = host();
    chat.pendingMediaUploading = true;
    await attachClipboardImages(chat);
    expect(chat.setError).toHaveBeenCalled();
    chat.pendingMediaUploading = false;
    chat.pendingMediaRefs = Array(MAX_MEDIA_REFS_PER_TURN).fill(ref);
    await attachClipboardImages(chat);
    expect(mocks.readClipboardImages).not.toHaveBeenCalled();
    chat.pendingMediaRefs.pop();
    await attachDroppedFiles(chat, [new File([], "one.png"), new File([], "two.png")]);
    expect(chat.setError).toHaveBeenLastCalledWith("Only 1 more attachment can be added to this message.");
    expect(mocks.uploadChatFiles).not.toHaveBeenCalled();
  });
});
