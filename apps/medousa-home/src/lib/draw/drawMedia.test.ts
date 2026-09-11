import { beforeEach, describe, expect, it, vi } from "vitest";
import { createDrawBrush, createEmptyDrawDocument } from "./drawDocument";
import { drawDocumentFromBytes, drawDocumentSvg, uploadDrawingMedia } from "./drawMedia";
import { serializeDrawDocumentJson } from "./drawDocument";

const { uploadMediaBytes } = vi.hoisted(() => ({ uploadMediaBytes: vi.fn() }));
vi.mock("$lib/daemon", () => ({ uploadMediaBytes }));

describe("drawMedia", () => {
  beforeEach(() => uploadMediaBytes.mockReset());

  it("renders retained pressure ink into a self-contained SVG preview", () => {
    const document = createEmptyDrawDocument();
    document.strokes.push({
      id: "ink<&\"",
      color: "#38bdf8",
      input: "pen",
      brush: createDrawBrush("pen", 8),
      points: [
        { x: 10, y: 12, elapsedMs: 0, pressure: 0.2 },
        { x: 80, y: 96, elapsedMs: 18, pressure: 0.9 },
      ],
    });

    const svg = drawDocumentSvg(document);
    expect(svg).toContain(`<svg xmlns="http://www.w3.org/2000/svg"`);
    expect(svg).toContain(`viewBox="0 0 ${document.width} ${document.height}"`);
    expect(svg).toContain(`<path d="`);
    expect(svg).toContain(`fill="#38bdf8"`);
  });

  it("restores the editable drawing source from daemon bytes", () => {
    const document = createEmptyDrawDocument();
    const bytes = new TextEncoder().encode(serializeDrawDocumentJson(document));
    expect(drawDocumentFromBytes(bytes)).toEqual(document);
  });

  it("uploads editable ink and its preview through daemon bytes only", async () => {
    const drawDocument = createEmptyDrawDocument();
    drawDocument.strokes.push({
      id: "ink",
      color: "#fff",
      input: "pen",
      brush: createDrawBrush("pen", 6),
      points: [{ x: 10, y: 20, pressure: 0.5 }],
    });
    uploadMediaBytes
      .mockResolvedValueOnce({ media_id: "usr:session:source", mime: "application/vnd.medousa.draw+json", byte_size: 10 })
      .mockResolvedValueOnce({ media_id: "usr:session:preview", mime: "image/png", byte_size: 20 });

    const originalImage = globalThis.Image;
    const originalCreateObjectUrl = URL.createObjectURL;
    const originalRevokeObjectUrl = URL.revokeObjectURL;
    const originalCreateElement = globalThis.document.createElement.bind(globalThis.document);
    class LoadedImage {
      decoding = "auto";
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(_value: string) { queueMicrotask(() => this.onload?.()); }
    }
    vi.stubGlobal("Image", LoadedImage);
    URL.createObjectURL = vi.fn(() => "blob:drawing");
    URL.revokeObjectURL = vi.fn();
    const canvas = {
      width: 0,
      height: 0,
      getContext: () => ({ drawImage: vi.fn() }),
      toBlob: (callback: (blob: Blob | null) => void) => callback(new Blob(["png"], { type: "image/png" })),
    };
    const createElement = vi.spyOn(globalThis.document, "createElement").mockImplementation(
      ((tag: string) => tag === "canvas" ? canvas : originalCreateElement(tag)) as typeof globalThis.document.createElement,
    );
    try {
      await expect(uploadDrawingMedia("session-a", drawDocument, "Sketch")).resolves.toEqual({
        media_id: "usr:session:preview",
        source_media_id: "usr:session:source",
        kind: "drawing",
        mime: "image/png",
        label: "Sketch",
      });
      expect(uploadMediaBytes).toHaveBeenCalledTimes(2);
      expect(uploadMediaBytes.mock.calls.map((call) => call[0])).toEqual(["session-a", "session-a"]);
    } finally {
      vi.stubGlobal("Image", originalImage);
      URL.createObjectURL = originalCreateObjectUrl;
      URL.revokeObjectURL = originalRevokeObjectUrl;
      createElement.mockRestore();
    }
  });
});
// @vitest-environment happy-dom
