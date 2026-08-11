// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from "vitest";

const { uploadMediaBytes, uploadMediaPath } = vi.hoisted(() => ({
  uploadMediaBytes: vi.fn(),
  uploadMediaPath: vi.fn(),
}));

vi.mock("$lib/daemon", () => ({
  uploadMediaBytes,
  uploadMediaPath,
}));

import {
  pickChatAttachmentFiles,
  uploadChatFiles,
  uploadChatPaths,
} from "$lib/utils/chatMediaUpload";

describe("chat media upload", () => {
  beforeEach(() => {
    uploadMediaBytes.mockReset();
    uploadMediaPath.mockReset();
    document.body.innerHTML = "";
  });

  it("creates and clicks the picker input synchronously", async () => {
    const click = vi.spyOn(HTMLInputElement.prototype, "click");

    const picked = pickChatAttachmentFiles();
    const input = document.querySelector<HTMLInputElement>('input[type="file"]');

    expect(input).not.toBeNull();
    expect(click).toHaveBeenCalledOnce();
    expect(input?.multiple).toBe(true);

    const file = new File(["hello"], "notes.txt", { type: "text/plain" });
    Object.defineProperty(input, "files", { value: [file] });
    input?.dispatchEvent(new Event("change"));

    await expect(picked).resolves.toEqual([file]);
    expect(document.body.contains(input)).toBe(false);
    click.mockRestore();
  });

  it("uploads dropped or picked files through the same byte path", async () => {
    uploadMediaBytes.mockResolvedValue({
      media_id: "usr:session-1:image",
      mime: "image/png",
      byte_size: 3,
      label: "pixel.png",
    });
    const file = new File([new Uint8Array([1, 2, 3])], "pixel.png", {
      type: "image/png",
    });

    const refs = await uploadChatFiles("session-1", [file]);

    expect(uploadMediaBytes).toHaveBeenCalledWith(
      "session-1",
      "pixel.png",
      "image/png",
      [1, 2, 3],
      "pixel.png",
    );
    expect(refs).toEqual([
      {
        media_id: "usr:session-1:image",
        kind: "image",
        mime: "image/png",
        label: "pixel.png",
      },
    ]);
  });

  it("adds the filename to upload failures", async () => {
    uploadMediaBytes.mockRejectedValue(new Error("file too large"));
    const file = new File([new Uint8Array([1])], "huge.png", {
      type: "image/png",
    });

    await expect(uploadChatFiles("session-1", [file])).rejects.toThrow(
      '"huge.png" — That file is too large',
    );
  });

  it("uploads Tauri drops from native paths", async () => {
    uploadMediaPath.mockResolvedValue({
      media_id: "usr:session-1:native-image",
      mime: "image/png",
      byte_size: 3,
      label: "rocket.png",
    });

    const refs = await uploadChatPaths("session-1", ["/tmp/rocket.png"]);

    expect(uploadMediaPath).toHaveBeenCalledWith(
      "session-1",
      "/tmp/rocket.png",
      "rocket.png",
    );
    expect(refs[0]).toMatchObject({
      media_id: "usr:session-1:native-image",
      kind: "image",
      label: "rocket.png",
    });
  });
});
