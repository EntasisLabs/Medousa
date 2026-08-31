// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from "vitest";

const { convertHeic, freeDecoder } = vi.hoisted(() => ({
  convertHeic: vi.fn(),
  freeDecoder: vi.fn(),
}));

vi.mock("@keeratita/heic-converter", () => ({
  convertHeic,
  LibheifDecoder: class LibheifDecoder {
    free = freeDecoder;
  },
}));

vi.mock("@keeratita/heic-converter/wasm?url", () => ({
  default: "/assets/heic-decoder.wasm",
}));

import {
  imageMimeFromBytes,
  normalizeChatUploadFile,
} from "$lib/utils/chatImageNormalization";
import { nativePathNeedsImageNormalization } from "$lib/utils/chatImageFormats";

describe("chat image normalization", () => {
  beforeEach(() => {
    convertHeic.mockReset();
    freeDecoder.mockReset();
  });

  it("sniffs browser-safe and ISO BMFF image formats", () => {
    expect(imageMimeFromBytes(new Uint8Array([0xff, 0xd8, 0xff, 0x00]))).toBe(
      "image/jpeg",
    );
    expect(
      imageMimeFromBytes(
        new Uint8Array([
          0, 0, 0, 24, 102, 116, 121, 112, 104, 101, 105, 99, 0, 0, 0, 0, 109,
          105, 102, 49,
        ]),
      ),
    ).toBe("image/heic");
  });

  it("repairs missing MIME metadata on a PNG upload", async () => {
    const file = new File(
      [new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])],
      "camera-upload",
    );

    const normalized = await normalizeChatUploadFile(file);

    expect(normalized.type).toBe("image/png");
    expect(normalized.name).toBe("camera-upload");
  });

  it("converts iPhone HEIC photos to JPEG and updates the filename", async () => {
    convertHeic.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: "image/jpeg" }));
    const file = new File(
      [
        new Uint8Array([
          0, 0, 0, 24, 102, 116, 121, 112, 104, 101, 105, 99, 0, 0, 0, 0, 109,
          105, 102, 49,
        ]),
      ],
      "IMG_4809.HEIC",
      { type: "image/heic" },
    );

    const normalized = await normalizeChatUploadFile(file);

    expect(convertHeic).toHaveBeenCalledOnce();
    expect(freeDecoder).toHaveBeenCalledOnce();
    expect(normalized.name).toBe("IMG_4809.jpg");
    expect(normalized.type).toBe("image/jpeg");
  });

  it("releases the HEIC decoder when conversion fails", async () => {
    convertHeic.mockRejectedValue(new Error("decode failed"));
    const file = new File(
      [
        new Uint8Array([
          0, 0, 0, 24, 102, 116, 121, 112, 104, 101, 105, 99, 0, 0, 0, 0, 109,
          105, 102, 49,
        ]),
      ],
      "broken.heic",
      { type: "image/heic" },
    );

    await expect(normalizeChatUploadFile(file)).rejects.toThrow(
      "Couldn't convert this iPhone photo",
    );
    expect(freeDecoder).toHaveBeenCalledOnce();
  });

  it("routes native desktop formats that need conversion through the byte pipeline", () => {
    expect(nativePathNeedsImageNormalization("/tmp/IMG_1.HEIF")).toBe(true);
    expect(nativePathNeedsImageNormalization("/tmp/photo.avif")).toBe(true);
    expect(nativePathNeedsImageNormalization("/tmp/photo.png")).toBe(false);
  });
});
