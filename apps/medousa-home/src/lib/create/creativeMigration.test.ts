import { describe, expect, it } from "vitest";
import { createEmptyDrawDocument, createDrawBrush, parseDrawDocumentJson } from "$lib/draw/drawDocument";
import { creativeAdoptionId, creativeSceneFromDraw, parseCreativeReference, serializeCreativeReference } from "./creativeMigration";
describe("creative migration", () => {
  it("retains pen data, out-of-frame coordinates and source identity without mutating the note", () => {
    const document = createEmptyDrawDocument();
    document.strokes = [{ id: "stroke_one", color: "#123456", input: "pen", brush: createDrawBrush("pencil"), points: [{ x: -25, y: 900, pressure: 0.3, elapsedMs: 23, tiltX: 4, tiltY: -5 }] }];
    const source = JSON.stringify(document); const scene = creativeSceneFromDraw(document);
    expect(JSON.stringify(document)).toBe(source); expect(scene.version).toBe(3); expect(scene.nodes[1].id).toBe("stroke_one");
    expect(scene.nodes[1].content).toMatchObject({ kind: "ink", points: [{ x: -25, y: 900, pressure: 0.3, elapsed_ms: 23, tilt_x: 4, tilt_y: -5 }] });
    expect(creativeSceneFromDraw(document)).toEqual(scene);
  });
  it("keeps the established v1 brush migration before adopting the scene", () => {
    const source = JSON.stringify({ schema: "medousa-draw", version: 1, width: 1200, height: 720, background: "transparent", strokes: [{ id: "one", color: "#123456", width: 8, points: [{ x: 50, y: 75 }] }] });
    const scene = creativeSceneFromDraw(parseDrawDocumentJson(source)); expect(scene.nodes[1].content).toMatchObject({ kind: "ink", brush: { size: 8, thinning: 0, smoothing: 0, streamline: 0, opacity: 1 } });
  });
  it("scopes adoption identities to workshops and preserves a reusable reference", async () => {
    const one = await creativeAdoptionId("one", "vault", "drawing.md", "drawing_one"); expect(await creativeAdoptionId("one", "vault", "drawing.md", "drawing_one")).toBe(one);
    expect(await creativeAdoptionId("two", "vault", "drawing.md", "drawing_one")).not.toBe(one);
    const ref = { document_id: one, frame_id: "legacy_frame" }; const fence = serializeCreativeReference(ref); expect(parseCreativeReference(fence.split("\n")[1])).toEqual(ref);
  });
  it("rejects paths, URLs, unknown metadata and oversized references", () => {
    for (const value of [{ document_id: "../outside" }, { document_id: "https://bad.invalid" }, { document_id: "doc", extra: true }]) expect(() => parseCreativeReference(JSON.stringify(value))).toThrow();
    expect(() => parseCreativeReference(" ".repeat(513))).toThrow("too large");
  });
});

it("matches the shared v3 fixture consumed by the Rust store", async () => {
  const { readFileSync } = await import("node:fs");
  const legacy = readFileSync(new URL("../../../../../tests/fixtures/creative/legacy-v2.json", import.meta.url), "utf8");
  const expected = JSON.parse(readFileSync(new URL("../../../../../tests/fixtures/creative/v3-from-legacy.json", import.meta.url), "utf8"));
  expect(creativeSceneFromDraw(parseDrawDocumentJson(legacy))).toEqual(expected);
});
