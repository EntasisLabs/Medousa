import { describe, expect, it } from "vitest";
import {
  MAX_DRAW_PAYLOAD_BYTES,
  createDrawBrush,
  createEmptyDrawDocument,
  decodeDrawDocument,
  drawDocumentFromContent,
  encodeDrawDocument,
  findDrawFence,
  noteHasDraw,
  replaceDrawFence,
  serializeDrawFence,
} from "./drawDocument";

function base64url(value: unknown): string {
  const binary = unescape(encodeURIComponent(JSON.stringify(value)));
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

describe("drawDocument", () => {
  it("round-trips a versioned vector scene through base64url", () => {
    const document = createEmptyDrawDocument();
    document.strokes.push({
      id: "ink-1",
      color: "#38bdf8",
      input: "pen",
      brush: createDrawBrush("pen", 6),
      points: [
        { x: 10, y: 20, elapsedMs: 0, pressure: 0.2 },
        { x: 30.5, y: 42.5, elapsedMs: 12, pressure: 0.8, tiltX: 12, tiltY: -8 },
      ],
    });

    const encoded = encodeDrawDocument(document);
    expect(encoded).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(decodeDrawDocument(encoded)).toEqual(document);
  });

  it("migrates a version-1 scene and keeps its visible stroke inputs", () => {
    const legacy = {
      schema: "medousa-draw",
      version: 1,
      width: 1200,
      height: 720,
      background: "transparent",
      strokes: [
        {
          id: "legacy",
          color: "#fff",
          width: 12,
          points: [{ x: 1, y: 2 }, { x: 3, y: 4, pressure: 0.7 }],
        },
      ],
    };
    const fence = `\`\`\`draw\nversion: 1\nencoding: base64url\npayload:\n  ${base64url(legacy)}\n\`\`\``;
    const migrated = findDrawFence(fence)?.document;

    expect(migrated?.version).toBe(2);
    expect(migrated?.strokes[0]).toMatchObject({
      id: "legacy",
      color: "#fff",
      input: "unknown",
      brush: { kind: "pen", size: 12, thinning: 0, smoothing: 0, streamline: 0, opacity: 1 },
      points: [{ x: 1, y: 2 }, { x: 3, y: 4, pressure: 0.7 }],
    });
    expect(serializeDrawFence(migrated!)).toContain("version: 2");
  });

  it("stores the scene in a self-describing Markdown fence", () => {
    const fence = serializeDrawFence(createEmptyDrawDocument());
    expect(fence).toContain("```draw\nversion: 2\nencoding: base64url\npayload:\n");
    expect(findDrawFence(fence)?.document.schema).toBe("medousa-draw");
    expect(noteHasDraw(fence)).toBe(true);
  });

  it("preserves subpixel scene widths used to keep brushes stable while zoomed in", () => {
    const document = createEmptyDrawDocument();
    document.strokes.push({
      id: "zoomed-ink",
      color: "#fff",
      input: "pen",
      brush: createDrawBrush("pen", 0.375),
      points: [{ x: 4, y: 8 }],
    });
    expect(decodeDrawDocument(encodeDrawDocument(document)).strokes[0].brush.size).toBe(0.375);
  });

  it("replaces only the drawing fence and preserves the surrounding note", () => {
    const initial = `# Idea\n\nBefore\n\n${serializeDrawFence(createEmptyDrawDocument())}\n\nAfter\n`;
    const document = drawDocumentFromContent(initial);
    document.strokes.push({
      id: "s",
      color: "#fff",
      input: "mouse",
      brush: createDrawBrush("pencil", 3),
      points: [{ x: 1, y: 2 }],
    });
    const next = replaceDrawFence(initial, document);
    expect(next).toContain("# Idea\n\nBefore");
    expect(next).toContain("\n\nAfter\n");
    expect(drawDocumentFromContent(next).strokes).toHaveLength(1);
  });

  it("rejects empty, malformed, unsupported, and oversized payloads", () => {
    expect(() => decodeDrawDocument("")).toThrow("empty");
    expect(() => decodeDrawDocument("bm90LWpzb24")).toThrow();
    expect(() =>
      decodeDrawDocument(base64url({ schema: "medousa-draw", version: 99 })),
    ).toThrow("Unsupported drawing version");
    expect(() =>
      decodeDrawDocument("a".repeat(Math.ceil((MAX_DRAW_PAYLOAD_BYTES * 4) / 3) + 9)),
    ).toThrow("too large");
  });
});
