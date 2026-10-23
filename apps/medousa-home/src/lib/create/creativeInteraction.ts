import type { CreativeNode } from "$lib/types/generated/daemon_api";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { lineGeometry, hitLine, setLineEndpoints, type LineStylePatch } from "./creativeLine";
import {
  createNode,
  editableNode,
  nodeBounds,
  nodeDimensions,
  selectionRoots,
  inverse,
  mapPoint,
  worldMatrix,
  parentPoint,
  resizeNode,
} from "./creativeScene";
import { remapTextSpans } from "./creativeRichText";
import { layoutText, type CreativeText, type TextStylePatch } from "./creativeText";
import { styleAppearance, roundedDistance, type AppearancePatch } from "./creativeAppearance";
import { randomUuid } from "$lib/utils/randomUuid";

export type CanvasShape = "rectangle" | "ellipse" | "line" | "arrow";
export type CanvasFill = "none" | "soft" | "solid";
export type CanvasSize = "s" | "m" | "l" | "xl";
export const canvasSizes = {
  s: { stroke: 2, text: 20 },
  m: { stroke: 4, text: 32 },
  l: { stroke: 7, text: 48 },
  xl: { stroke: 12, text: 72 },
} as const;
export const canvasColors = [
  { name: "Ink", value: "#393e33" },
  { name: "Gray", value: "#83918a" },
  { name: "Purple", value: "#9474dc" },
  { name: "Blue", value: "#4e75d9" },
  { name: "Teal", value: "#4f9eaf" },
  { name: "Amber", value: "#d79737" },
  { name: "Green", value: "#3b9a70" },
  { name: "Pink", value: "#d16d8e" },
  { name: "Red", value: "#d76956" },
];
/** The default ink stays legible on both the workspace and opaque artboards. */
export function canvasInk(
  background: string | undefined,
  fallback: string,
): string {
  if (!background || background === "transparent") return fallback;
  const rgb = /^#[\da-f]{3}$/i.test(background)
    ? "#" + [...background.slice(1)].map((c) => c + c).join("")
    : background;
  if (!/^#[\da-f]{6}$/i.test(rgb)) return fallback;
  const values = [1, 3, 5].map((i) => parseInt(rgb.slice(i, i + 2), 16));
  return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722 > 145
    ? "#393e33"
    : "#e7e6df";
}
export function shapeFill(color: string, fill: CanvasFill) {
  const rgb = /^#[\da-f]{3,4}$/i.test(color)
    ? "#" + [...color.slice(1, 4)].map((c) => c + c).join("")
    : color.slice(0, 7);
  return fill === "none" ? "transparent" : fill === "soft" ? `${rgb}24` : color;
}
export function nodeColor(node: CreativeNode): string | null {
  const c = node.content;
  const line = lineGeometry(node);
  if (line) return line.color;
  return c.kind === "ink" || c.kind === "text"
    ? c.color
    : c.kind === "shape"
      ? c.stroke !== "transparent" && c.stroke_width > 0
        ? c.stroke
        : c.fill
      : c.kind === "frame"
        ? c.background
        : null;
}
export function nodeFill(node: CreativeNode): CanvasFill {
  const c = node.content;
  if (lineGeometry(node)) return "none";
  return c.kind !== "shape" || c.fill === "transparent"
    ? "none"
    : /^#(?:[\da-f]{4}|[\da-f]{8})$/i.test(c.fill)
      ? "soft"
      : "solid";
}
/** Hit strokes themselves, so erasing empty space inside a sketch is safe. */
export function hitCanvasNode(
  node: CreativeNode,
  nodes: CreativeNode[],
  point: DrawVector,
  tolerance: number,
): boolean {
  if (
    !editableNode(node, nodes) ||
    node.content.kind === "frame" ||
    node.content.kind === "group"
  )
    return false;
  const p = mapPoint(inverse(worldMatrix(node, nodes)), point),
    c = node.content;
  // Rounded frames clip their descendants in the renderer, including picking.
  let parent = nodes.find(n => n.id === node.parent_id);
  while (parent) {
    if (parent.content.kind === "frame" && roundedDistance(parent, mapPoint(inverse(worldMatrix(parent, nodes)), point)) > 0) return false;
    parent = nodes.find(n => n.id === parent!.parent_id);
  }
  if (c.kind === "image") return roundedDistance(node, p) <= tolerance;
  const segment = (a: DrawVector, b: DrawVector) => {
    const dx = b.x - a.x,
      dy = b.y - a.y,
      length = dx * dx + dy * dy;
    const t = length
      ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length))
      : 0;
    return Math.hypot(p.x - a.x - dx * t, p.y - a.y - dy * t);
  };
  const connector = lineGeometry(node);
  if (connector) return hitLine(connector, p, tolerance);
  if (c.kind === "ink")
    return c.points.some(
      (a, i) =>
        segment(a, c.points[Math.min(i + 1, c.points.length - 1)]) <=
        tolerance + c.brush.size / 2,
    );
  const { width: w, height: h } = nodeDimensions(node);
  if (c.kind === "shape") {
    const pad = tolerance + c.stroke_width / 2;
    if (c.shape === "line")
      return segment({ x: 0, y: 0 }, { x: w, y: h }) <= pad;
    if (c.shape === "ellipse") {
      const radius = Math.hypot(
        (p.x - w / 2) / (w / 2),
        (p.y - h / 2) / (h / 2),
      );
      return (
        radius <= 1 + pad / Math.min(w / 2, h / 2) &&
        (c.fill !== "transparent" || radius >= 1 - pad / Math.min(w / 2, h / 2))
      );
    }
    const distance = roundedDistance(node, p);
    return distance <= pad && (c.fill !== "transparent" || distance >= -pad);
  }
  return (
    p.x >= -tolerance &&
    p.y >= -tolerance &&
    p.x <= w + tolerance &&
    p.y <= h + tolerance
  );
}
export function nodeSize(node: CreativeNode): CanvasSize {
  const c = node.content,
    value =
      c.kind === "text"
        ? c.font_size
        : c.kind === "ink"
          ? c.brush.size
          : c.kind === "shape"
            ? c.stroke_width
            : 4;
  return (Object.keys(canvasSizes) as CanvasSize[]).reduce((best, size) => {
    const key = c.kind === "text" ? "text" : "stroke";
    return Math.abs(canvasSizes[size][key] - value) <
      Math.abs(canvasSizes[best][key] - value)
      ? size
      : best;
  }, "m");
}
export type CanvasStylePatch = { color?: string; size?: CanvasSize; fill?: CanvasFill } & TextStylePatch & LineStylePatch & AppearancePatch;
export function updateTextNode(node: CreativeNode, patch: Partial<Omit<CreativeText, "kind">>): CreativeNode {
  if (node.content.kind !== "text") return node;
  const content = { ...node.content, ...patch };
  if (patch.text !== undefined && patch.text !== node.content.text && node.text_layout?.spans?.length) node = { ...node, text_layout: { ...node.text_layout, spans: remapTextSpans(node.content.text, patch.text, node.text_layout.spans) } };
  const bounds = layoutText(content, undefined, node.text_layout);
  return { ...resizeNode(node, bounds.width, bounds.height), content };
}
export function styleNode(
  node: CreativeNode,
  patch: CanvasStylePatch,
): CreativeNode {
  node = styleAppearance(node, patch);
  const c = node.content,
    color = patch.color ?? nodeColor(node) ?? canvasColors[0].value;
  const connector = lineGeometry(node);
  if (connector) return {
    ...node, opacity: node.opacity * connector.opacity,
    content: { kind: "shape", shape: "line", fill: "transparent", stroke: color,
      stroke_width: patch.stroke_width !== undefined && Number.isFinite(patch.stroke_width) ? Math.max(.5, Math.min(100, patch.stroke_width)) : patch.size ? canvasSizes[patch.size].stroke : connector.stroke_width,
      line: { start: connector.start, end: connector.end, start_arrow: patch.start_arrow ?? connector.start_arrow, end_arrow: patch.end_arrow ?? connector.end_arrow, dashed: patch.dashed ?? connector.dashed },
    },
  };
  if (c.kind === "ink")
    return {
      ...node,
      content: {
        ...c,
        color,
        brush: {
          ...c.brush,
          ...(patch.size ? { size: canvasSizes[patch.size].stroke } : {}),
        },
      },
    };
  if (c.kind === "text") {
    const text_layout = { ...node.text_layout, width: node.text_layout?.width ?? null, align: node.text_layout?.align ?? "left" as const, line_height: node.text_layout?.line_height ?? 1.3,
      ...(patch.text_width !== undefined ? { width: patch.text_width } : {}), ...(patch.text_align ? { align: patch.text_align } : {}), ...(patch.line_height !== undefined ? { line_height: patch.line_height } : {}) };
    for (const key of ["letter_spacing", "paragraph_spacing", "underline"] as const) if (patch[key] !== undefined) Object.assign(text_layout, { [key]: patch[key] });
    if (text_layout.spans?.length) text_layout.spans = text_layout.spans.map(span => ({ ...span, ...(patch.font_weight !== undefined ? { font_weight: null } : {}), ...(patch.font_style !== undefined ? { font_style: null } : {}), ...(patch.color !== undefined ? { color: null } : {}), ...(patch.underline !== undefined ? { underline: null } : {}) }));
    return updateTextNode({ ...node, ...(node.text_layout || patch.text_width !== undefined || patch.text_align || patch.line_height !== undefined || patch.letter_spacing !== undefined || patch.paragraph_spacing !== undefined || patch.underline !== undefined ? { text_layout } : {}) }, {
      color,
      ...(patch.size ? { font_size: canvasSizes[patch.size].text } : {}),
      ...(patch.font_size !== undefined && Number.isFinite(patch.font_size) ? { font_size: Math.max(4, Math.min(512, patch.font_size)) } : {}),
      ...(patch.font_family ? { font_family: patch.font_family } : {}),
      ...(patch.font_weight !== undefined ? { font_weight: patch.font_weight } : {}),
      ...(patch.font_style !== undefined ? { font_style: patch.font_style } : {}),
    });
  }
  if (c.kind === "frame" && patch.color)
    return { ...node, content: { ...c, background: color } };
  if (c.kind === "shape")
    return {
      ...node,
      content: {
        ...c,
        fill:
          c.shape === "line"
            ? "transparent"
            : patch.color || patch.fill ? shapeFill(color, patch.fill ?? nodeFill(node)) : c.fill,
        stroke: patch.color ?? c.stroke,
        stroke_width: patch.size
          ? canvasSizes[patch.size].stroke
          : c.stroke_width,
      },
    };
  return node;
}
export function dragRect(a: DrawVector, b: DrawVector): DrawRect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}
/** Fully contained editable objects, excluding frames unless explicitly selected. */
export function marqueeSelection(
  nodes: CreativeNode[],
  rect: DrawRect,
  initial: string[] = [],
): string[] {
  const ids = nodes
    .filter((n) => {
      if (n.content.kind === "frame" || !editableNode(n, nodes)) return false;
      const b = nodeBounds(n, nodes);
      return (
        b.x >= rect.x &&
        b.y >= rect.y &&
        b.x + b.width <= rect.x + rect.width &&
        b.y + b.height <= rect.y + rect.height
      );
    })
    .map((n) => n.id);
  return selectionRoots([...new Set([...initial, ...ids])], nodes).map(
    (n) => n.id,
  );
}
/** Line and Arrow share exact endpoint geometry and the existing shape transaction. */
export function dragShape(seed: CreativeNode, shape: CanvasShape, a: DrawVector, b: DrawVector, color: string, size: number, fill: CanvasFill, lineStyle: LineStylePatch = {}): CreativeNode {
  if (shape === "line" || shape === "arrow") {
    const base = { ...seed, content: { kind: "shape" as const, shape: "line" as const, fill: "transparent", stroke: color, stroke_width: size } };
    return setLineEndpoints(base, a, b, { stroke_width: size, ...lineStyle, end_arrow: lineStyle.end_arrow ?? shape === "arrow" });
  }
  const rect = dragRect(a, b);
  return { ...seed, transform: { ...rect, width: Math.max(1, rect.width), height: Math.max(1, rect.height), rotation: 0 },
    content: { kind: "shape", shape, fill: shapeFill(color, fill), stroke: color, stroke_width: size } };
}
/** Duplicate a selection with its descendants and remap their parent IDs atomically. */
export function duplicateSelection(
  ids: string[],
  nodes: CreativeNode[],
  offset: DrawVector,
): CreativeNode[] {
  const roots = selectionRoots(ids, nodes),
    wanted = new Set(roots.map((n) => n.id));
  for (let changed = true; changed;) {
    changed = false;
    for (const n of nodes)
      if (n.parent_id && wanted.has(n.parent_id) && !wanted.has(n.id)) {
        wanted.add(n.id);
        changed = true;
      }
  }
  const remap = new Map([...wanted].map((id) => [id, randomUuid()]));
  return nodes
    .filter((n) => wanted.has(n.id))
    .map((n) => {
      const a = parentPoint(n, nodes, { x: 0, y: 0 }),
        b = parentPoint(n, nodes, offset);
      return {
        ...structuredClone(n),
        id: remap.get(n.id)!,
        parent_id: remap.get(n.parent_id ?? "") ?? n.parent_id,
        transform: {
          ...n.transform,
          x: n.transform.x + (roots.includes(n) ? b.x - a.x : 0),
          y: n.transform.y + (roots.includes(n) ? b.y - a.y : 0),
        },
      };
    });
}
export function newCanvasText(
  point: DrawVector,
  color: string,
  size: CanvasSize,
  parentId: string | null,
  textStyle: TextStylePatch = {},
): CreativeNode {
  const node = createNode(
    {
      kind: "text",
      text: "",
      color,
      font_family: textStyle.font_family ?? "sans-serif",
      font_size: textStyle.font_size ?? canvasSizes[size].text,
      font_weight: textStyle.font_weight, font_style: textStyle.font_style,
    },
    { ...point, width: 240, height: canvasSizes[size].text * 1.3 },
    parentId,
  );
  return styleNode(node, textStyle);
}
