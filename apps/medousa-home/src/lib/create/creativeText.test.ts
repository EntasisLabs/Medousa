import { expect, it } from "vitest";
import { layoutText, textFont } from "./creativeText";
import { createNode, mapPoint, worldMatrix } from "./creativeScene";
import { hitCanvasNode, styleNode, updateTextNode } from "./creativeInteraction";

const content = { kind: "text", text: "Wide\n\nT", font_family: "serif", font_size: 40, color: "#000", font_weight: 700, font_style: "italic" } as const;
it("measures line advances and italic overhang, preserving empty lines", () => {
  const layout = layoutText(content, text => ({ width: text.length * 11, actualBoundingBoxLeft: 3, actualBoundingBoxRight: text.length * 11 + 2, fontBoundingBoxAscent: 32, fontBoundingBoxDescent: 8 }));
  expect(layout.lines).toEqual(["Wide", "", "T"]);
  expect(layout.width).toBeCloseTo(46 + 3 + 4.8);
  expect(layout.height).toBeCloseTo(3 * 52 + 4.8);
  expect(layout.baseline).toBeCloseTo(2.4 + 6 + 32);
  expect(layout.insetX).toBeCloseTo(5.4);
  expect(textFont(content)).toBe("italic 700 40px serif");
});
it("grows and shrinks text around its original rotated origin, and removes stale hit areas", () => {
  const node = createNode({ ...content, text: "Short" }, { x: 50, y: 90, width: 900, height: 20 });
  node.transform.rotation = 25;
  const before = mapPoint(worldMatrix(node, [node]), { x: 0, y: 0 });
  const larger = styleNode(node, { font_size: 80, font_family: "monospace", font_weight: 400, font_style: "normal" });
  const smaller = updateTextNode(styleNode(larger, { font_size: 20 }), { text: "Hi" });
  const after = mapPoint(worldMatrix(smaller, [smaller]), { x: 0, y: 0 });
  expect(after.x).toBeCloseTo(before.x); expect(after.y).toBeCloseTo(before.y);
  expect(smaller.transform.width).toBeLessThan(larger.transform.width);
  expect(smaller.transform.height).toBeLessThan(larger.transform.height);
  expect(hitCanvasNode(smaller, [smaller], mapPoint(worldMatrix(smaller, [smaller]), { x: 400, y: 5 }), 0)).toBe(false);
  expect(smaller.content).toMatchObject({ text: "Hi", font_family: "monospace", font_size: 20, font_weight: 400, font_style: "normal" });
});
