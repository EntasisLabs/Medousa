// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { canReadClipboardImages, handleChatImagePaste, readClipboardImages } from "./chatImagePaste";

const file = () => new File(["image"], "shot.png", { type: "image/png" });
function paste(files: File[], text = "", itemsOnly = false) {
  const event = new Event("paste", { cancelable: true, bubbles: true }) as ClipboardEvent;
  Object.defineProperty(event, "clipboardData", { value: {
    files: itemsOnly ? [] : files,
    items: files.map((file) => ({ kind: "file", type: file.type, getAsFile: () => file })),
    getData: (type: string) => type === "text/plain" ? text : "",
  } });
  return event;
}
function clipboard(items: Partial<ClipboardItem>[]) {
  const read = vi.fn().mockResolvedValue(items);
  vi.stubGlobal("navigator", { clipboard: { read } });
  return read;
}
afterEach(() => vi.unstubAllGlobals());

describe("composer image paste", () => {
  it("handles a paste event and attaches each image once", () => {
    const attach = vi.fn();
    const target = document.createElement("textarea");
    target.addEventListener("paste", (event) => handleChatImagePaste(event, { blocked: false, attach }));
    const images = [file(), file()];
    const event = paste(images);
    target.dispatchEvent(event);
    expect(attach).toHaveBeenCalledExactlyOnceWith(images);
    expect(event.defaultPrevented).toBe(true);
  });
  it("supports clipboard item files even when FileList is empty", () => {
    const image = file();
    const attach = vi.fn();
    handleChatImagePaste(paste([image], "", true), { blocked: false, attach });
    expect(attach).toHaveBeenCalledWith([image]);
  });
  it("leaves text-only and mixed text insertion to the textarea", () => {
    for (const files of [[], [file()]]) {
      const attach = vi.fn();
      const event = paste(files, "a caption");
      handleChatImagePaste(event, { blocked: false, attach });
      expect(event.defaultPrevented).toBe(false);
      expect(attach).toHaveBeenCalledTimes(files.length);
    }
  });
  it("does not attach to a disabled composer", () => {
    const attach = vi.fn();
    const event = paste([file()]);
    handleChatImagePaste(event, { blocked: true, attach });
    expect(event.defaultPrevented).toBe(true);
    expect(attach).not.toHaveBeenCalled();
  });
  it("reads synchronously in the click and chooses one image representation per item", async () => {
    const getType = vi.fn().mockResolvedValue(new Blob(["png"], { type: "image/png" }));
    const read = clipboard([{ types: ["text/html", "image/jpeg", "image/png"], getType }]);
    const pending = readClipboardImages(2);
    expect(read).toHaveBeenCalledOnce();
    const files = await pending;
    expect(getType).toHaveBeenCalledExactlyOnceWith("image/png");
    expect(files).toHaveLength(1);
    expect(files[0].name).toBe("Pasted image 1.png");
  });
  it("explains missing images and permission denial without reading HTML or text", async () => {
    const getType = vi.fn();
    const read = clipboard([{ types: ["text/html", "text/plain"], getType }]);
    await expect(readClipboardImages(2)).rejects.toThrow("No image");
    expect(getType).not.toHaveBeenCalled();
    read.mockRejectedValue(new DOMException("denied", "NotAllowedError"));
    await expect(readClipboardImages(2)).rejects.toThrow("Choose Paste if prompted");
  });
  it("rejects empty images and too many items", async () => {
    const getType = vi.fn().mockResolvedValue(new Blob([]));
    clipboard([{ types: ["image/png"], getType }]);
    await expect(readClipboardImages(2)).rejects.toThrow("empty");
    getType.mockResolvedValue(new Blob(["png"]));
    clipboard([{ types: ["image/png"], getType }, { types: ["image/png"], getType }]);
    await expect(readClipboardImages(1)).rejects.toThrow("Only 1 more image");
  });
  it("offers an actionable fallback when clipboard reads are unavailable", async () => {
    vi.stubGlobal("navigator", {});
    expect(canReadClipboardImages()).toBe(false);
    await expect(readClipboardImages(1)).rejects.toThrow("Use Paste in the message field");
  });
});
