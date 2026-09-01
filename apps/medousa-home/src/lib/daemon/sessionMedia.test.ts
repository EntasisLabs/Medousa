// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import {
  readMediaBytes,
  readMediaImagePath,
  uploadMediaBytes,
} from "$lib/daemon/session";

describe("session media IPC", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("sends upload bytes as compact base64", async () => {
    invoke.mockResolvedValue({
      media_id: "usr:session-1:image",
      mime: "image/png",
      byte_size: 3,
    });

    await uploadMediaBytes(
      "session-1",
      "pixel.png",
      "image/png",
      new Uint8Array([0, 255, 1]),
      "pixel.png",
    );

    expect(invoke).toHaveBeenCalledWith("media_upload", {
      sessionId: "session-1",
      filename: "pixel.png",
      mime: "image/png",
      bytesBase64: "AP8B",
      label: "pixel.png",
    });
  });

  it("decodes media and native-path responses back to bytes", async () => {
    invoke
      .mockResolvedValueOnce({ mime: "image/jpeg", bytes_base64: "AP8B" })
      .mockResolvedValueOnce({
        filename: "photo.heic",
        mime: "image/heic",
        bytes_base64: "AP8B",
      });

    await expect(readMediaBytes("session-1", "image-1")).resolves.toEqual({
      mime: "image/jpeg",
      bytes: new Uint8Array([0, 255, 1]),
    });
    await expect(readMediaImagePath("/tmp/photo.heic")).resolves.toEqual({
      filename: "photo.heic",
      mime: "image/heic",
      bytes: new Uint8Array([0, 255, 1]),
    });
  });
});
