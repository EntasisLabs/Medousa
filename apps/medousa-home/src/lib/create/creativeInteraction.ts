import type { CreativeNode } from "$lib/types/generated/daemon_api";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { createDrawBrush } from "$lib/draw/drawDocument";
import {
  createNode,
  editableNode,
  nodeBounds,
  selectionRoots,
  inverse,
  mapPoint,
  worldMatrix,
  parentPoint,
  resizeNode,
} from "./creativeScene";
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
  const segment = (a: DrawVector, b: DrawVector) => {
    const dx = b.x - a.x,
      dy = b.y - a.y,
      length = dx * dx + dy * dy;
    const t = length
      ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length))
      : 0;
    return Math.hypot(p.x - a.x - dx * t, p.y - a.y - dy * t);
  };
  if (c.kind === "ink")
    return c.points.some(
      (a, i) =>
        segment(a, c.points[Math.min(i + 1, c.points.length - 1)]) <=
        tolerance + c.brush.size / 2,
    );
  const { width: w, height: h } = node.transform;
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
    if (c.fill === "transparent")
      return [
        [
          { x: 0, y: 0 },
          { x: w, y: 0 },
        ],
        [
          { x: w, y: 0 },
          { x: w, y: h },
        ],
        [
          { x: w, y: h },
          { x: 0, y: h },
        ],
        [
          { x: 0, y: h },
          { x: 0, y: 0 },
        ],
      ].some(([a, b]) => segment(a, b) <= pad);
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
export function styleNode(
  node: CreativeNode,
  patch: { color?: string; size?: CanvasSize; fill?: CanvasFill },
): CreativeNode {
  const c = node.content,
    color = patch.color ?? nodeColor(node) ?? canvasColors[0].value;
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
    const ratio = patch.size ? canvasSizes[patch.size].text / c.font_size : 1;
    return {
      ...resizeNode(
        node,
        node.transform.width * ratio,
        node.transform.height * ratio,
      ),
      content: {
        ...c,
        color,
        ...(patch.size ? { font_size: canvasSizes[patch.size].text } : {}),
      },
    };
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
            : shapeFill(color, patch.fill ?? nodeFill(node)),
        stroke: color,
        stroke_width: patch.size
          ? canvasSizes[patch.size].stroke
          : Math.max(1, c.stroke_width),
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
/** Arrows use the existing editable ink representation; no new daemon schema. */
export function dragShape(
  seed: CreativeNode,
  shape: CanvasShape,
  a: DrawVector,
  b: DrawVector,
  color: string,
  size: number,
  fill: CanvasFill,
): CreativeNode {
  const rect = dragRect(a, b),
    width = Math.max(1, rect.width),
    height = Math.max(1, rect.height);
  if (shape === "arrow") {
    const angle = Math.atan2(b.y - a.y, b.x - a.x),
      head = Math.min(Math.hypot(b.x - a.x, b.y - a.y) * 0.35, 24 + size);
    const pts = [
      a,
      b,
      {
        x: b.x - Math.cos(angle - 0.5) * head,
        y: b.y - Math.sin(angle - 0.5) * head,
      },
      b,
      {
        x: b.x - Math.cos(angle + 0.5) * head,
        y: b.y - Math.sin(angle + 0.5) * head,
      },
    ];
    const bounds = dragRect(
      {
        x: Math.min(...pts.map((p) => p.x)),
        y: Math.min(...pts.map((p) => p.y)),
      },
      {
        x: Math.max(...pts.map((p) => p.x)),
        y: Math.max(...pts.map((p) => p.y)),
      },
    );
    return {
      ...seed,
      transform: {
        ...bounds,
        width: Math.max(1, bounds.width),
        height: Math.max(1, bounds.height),
        rotation: 0,
      },
      content: {
        kind: "ink",
        input: "mouse",
        color,
        brush: createDrawBrush("pen", size),
        points: pts.map((p) => ({
          x: p.x - bounds.x,
          y: p.y - bounds.y,
          pressure: 0.5,
        })),
      },
    };
  }
  const rotation =
    shape === "line"
      ? ((Math.atan2(b.y - a.y, b.x - a.x) - Math.atan2(height, width)) * 180) /
        Math.PI
      : 0;
  return {
    ...seed,
    transform: { ...rect, width, height, rotation },
    content: {
      kind: "shape",
      shape,
      fill: shape === "line" ? "transparent" : shapeFill(color, fill),
      stroke: color,
      stroke_width: size,
    },
  };
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
): CreativeNode {
  return createNode(
    {
      kind: "text",
      text: "",
      color,
      font_family: "sans-serif",
      font_size: canvasSizes[size].text,
    },
    { ...point, width: 240, height: canvasSizes[size].text * 1.3 },
    parentId,
  );
}
