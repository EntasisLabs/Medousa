import { describe, expect, it } from "vitest";
import { createDrawBrush, createEmptyDrawDocument, type DrawStroke } from "./drawDocument";
import { applyDrawPatch, createDrawPatch } from "./drawHistory";

function stroke(id: string, x: number): DrawStroke {
  return {
    id,
    color: "#fff",
    input: "pen",
    brush: createDrawBrush(),
    points: [{ x, y: 0 }],
  };
}

describe("draw history", () => {
  it("stores and replays only changed stroke snapshots", () => {
    const before = createEmptyDrawDocument();
    before.strokes = [stroke("stable", 1), stroke("moving", 2)];
    const after = { ...before, strokes: [before.strokes[0], stroke("moving", 20), stroke("new", 30)] };
    const patch = createDrawPatch(before, after, "Move and duplicate");
    expect(patch?.before.map((entry) => entry.stroke.id)).toEqual(["moving"]);
    expect(patch?.after.map((entry) => entry.stroke.id)).toEqual(["moving", "new"]);
    expect(applyDrawPatch(after, patch!, "undo")).toEqual(before);
    expect(applyDrawPatch(before, patch!, "redo")).toEqual(after);
  });
});
