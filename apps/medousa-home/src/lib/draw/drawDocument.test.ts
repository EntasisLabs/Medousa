import { describe, expect, it } from "vitest";
import {
  createEmptyDrawDocument,
  decodeDrawDocument,
  drawDocumentFromContent,
  encodeDrawDocument,
  findDrawFence,
  noteHasDraw,
  replaceDrawFence,
  serializeDrawFence,
} from "./drawDocument";

describe("drawDocument", () => {
  it("round-trips a versioned vector scene through base64url", () => {
    const document = createEmptyDrawDocument();
    document.strokes.push({
      id: "ink-1",
      color: "#38bdf8",
      width: 6,
      points: [{ x: 10, y: 20 }, { x: 30.5, y: 42.5, pressure: 0.8 }],
    });

    const encoded = encodeDrawDocument(document);
    expect(encoded).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(decodeDrawDocument(encoded)).toEqual(document);
  });

  it("stores the scene in a self-describing Markdown fence", () => {
    const fence = serializeDrawFence(createEmptyDrawDocument());
    expect(fence).toContain("```draw\nversion: 1\nencoding: base64url\npayload:\n");
    expect(findDrawFence(fence)?.document.schema).toBe("medousa-draw");
    expect(noteHasDraw(fence)).toBe(true);
  });

  it("replaces only the drawing fence and preserves the surrounding note", () => {
    const initial = `# Idea\n\nBefore\n\n${serializeDrawFence(createEmptyDrawDocument())}\n\nAfter\n`;
    const document = drawDocumentFromContent(initial);
    document.strokes.push({ id: "s", color: "#fff", width: 3, points: [{ x: 1, y: 2 }] });
    const next = replaceDrawFence(initial, document);
    expect(next).toContain("# Idea\n\nBefore");
    expect(next).toContain("\n\nAfter\n");
    expect(drawDocumentFromContent(next).strokes).toHaveLength(1);
  });
});
