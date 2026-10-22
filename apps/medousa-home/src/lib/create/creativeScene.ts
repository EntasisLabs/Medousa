import type {
  CreativeNode,
  CreativeOperation,
  CreativeScene,
} from "$lib/types/generated/daemon_api";
import { drawStrokeOutlinePath } from "$lib/draw/drawGeometry";
import {
  fitDrawCamera,
  viewToScene,
  zoomDrawCameraAt,
  type DrawCamera,
  type DrawRect,
  type DrawVector,
  type DrawSize,
} from "$lib/draw/drawCamera";
import { randomUuid } from "$lib/utils/randomUuid";

export type CreativeTool =
  "select" | "hand" | "ink" | "erase" | "text" | "shape" | "frame";
export type Matrix = [number, number, number, number, number, number];
const identity: Matrix = [1, 0, 0, 1, 0, 0];
export function multiply(a: Matrix, b: Matrix): Matrix {
  return [
    a[0] * b[0] + a[2] * b[1],
    a[1] * b[0] + a[3] * b[1],
    a[0] * b[2] + a[2] * b[3],
    a[1] * b[2] + a[3] * b[3],
    a[0] * b[4] + a[2] * b[5] + a[4],
    a[1] * b[4] + a[3] * b[5] + a[5],
  ];
}
export function mapPoint(m: Matrix, p: DrawVector): DrawVector {
  return {
    x: m[0] * p.x + m[2] * p.y + m[4],
    y: m[1] * p.x + m[3] * p.y + m[5],
  };
}
export function inverse(m: Matrix): Matrix {
  const d = m[0] * m[3] - m[1] * m[2];
  return [
    m[3] / d,
    -m[1] / d,
    -m[2] / d,
    m[0] / d,
    (m[2] * m[5] - m[3] * m[4]) / d,
    (m[1] * m[4] - m[0] * m[5]) / d,
  ];
}
export function localMatrix(node: CreativeNode): Matrix {
  const { x, y, width: w, height: h, rotation } = node.transform;
  const a = (rotation * Math.PI) / 180,
    c = Math.cos(a),
    s = Math.sin(a);
  return [
    c,
    s,
    -s,
    c,
    x + w / 2 - (c * w) / 2 + (s * h) / 2,
    y + h / 2 - (s * w) / 2 - (c * h) / 2,
  ];
}
export function worldMatrix(node: CreativeNode, nodes: CreativeNode[]): Matrix {
  const parent = nodes.find((n) => n.id === node.parent_id);
  return multiply(
    parent ? worldMatrix(parent, nodes) : identity,
    localMatrix(node),
  );
}
export function parentPoint(
  node: CreativeNode,
  nodes: CreativeNode[],
  point: DrawVector,
): DrawVector {
  const parent = nodes.find((n) => n.id === node.parent_id);
  return parent ? mapPoint(inverse(worldMatrix(parent, nodes)), point) : point;
}
export function nodeBounds(
  node: CreativeNode,
  nodes: CreativeNode[],
): DrawRect {
  const m = worldMatrix(node, nodes),
    { width, height } = node.transform;
  const points = [
    { x: 0, y: 0 },
    { x: width, y: 0 },
    { x: 0, y: height },
    { x: width, y: height },
  ].map((p) => mapPoint(m, p));
  const x = Math.min(...points.map((p) => p.x)),
    y = Math.min(...points.map((p) => p.y));
  return {
    x,
    y,
    width: Math.max(...points.map((p) => p.x)) - x,
    height: Math.max(...points.map((p) => p.y)) - y,
  };
}
export function sceneBounds(nodes: CreativeNode[]): DrawRect | null {
  const rects = nodes.filter((n) => n.visible).map((n) => nodeBounds(n, nodes));
  if (!rects.length) return null;
  const x = Math.min(...rects.map((r) => r.x)),
    y = Math.min(...rects.map((r) => r.y));
  return {
    x,
    y,
    width: Math.max(...rects.map((r) => r.x + r.width)) - x,
    height: Math.max(...rects.map((r) => r.y + r.height)) - y,
  };
}
export function nodeName(node: CreativeNode): string {
  if (node.name?.trim()) return node.name;
  const c = node.content;
  return c.kind === "frame"
    ? c.name
    : c.kind === "text"
      ? c.text.split("\n")[0].slice(0, 48) || "Text"
      : c.kind === "shape"
        ? c.shape
        : c.kind[0].toUpperCase() + c.kind.slice(1);
}
export function editableNode(
  node: CreativeNode,
  nodes: CreativeNode[],
): boolean {
  if (node.locked || !node.visible) return false;
  const parent = nodes.find((n) => n.id === node.parent_id);
  return !parent || editableNode(parent, nodes);
}
export function selectionRoots(
  ids: string[],
  nodes: CreativeNode[],
): CreativeNode[] {
  const selected = new Set(ids);
  return nodes.filter(
    (n) =>
      selected.has(n.id) && editableNode(n, nodes) && !hasSelectedParent(n),
  );
  function hasSelectedParent(node: CreativeNode): boolean {
    const p = nodes.find((n) => n.id === node.parent_id);
    return Boolean(p && (selected.has(p.id) || hasSelectedParent(p)));
  }
}
export function applySceneOperations(
  scene: CreativeScene,
  operations: CreativeOperation[],
): CreativeScene {
  let nodes = [...scene.nodes],
    background = scene.background;
  for (const op of operations) {
    if (op.op === "put_node") {
      const i = nodes.findIndex((n) => n.id === op.node.id);
      if (i < 0) nodes.push(op.node);
      else nodes[i] = op.node;
    }
    if (op.op === "remove_node") {
      const removed = new Set([op.id]);
      for (let changed = true; changed;) {
        changed = false;
        for (const n of nodes)
          if (n.parent_id && removed.has(n.parent_id) && !removed.has(n.id)) {
            removed.add(n.id);
            changed = true;
          }
      }
      nodes = nodes.filter((n) => !removed.has(n.id));
    }
    if (op.op === "reorder") {
      const byId = new Map(nodes.map((n) => [n.id, n]));
      nodes = op.ids
        .map((id) => byId.get(id))
        .filter((n): n is CreativeNode => Boolean(n));
    }
    if (op.op === "set_background") background = op.color;
  }
  return { ...scene, nodes, background };
}
export function createNode(
  content: CreativeNode["content"],
  rect: DrawRect,
  parentId: string | null = null,
): CreativeNode {
  return {
    id: randomUuid(),
    parent_id: parentId,
    transform: { ...rect, rotation: 0 },
    opacity: 1,
    visible: true,
    locked: false,
    content,
  };
}
export function inkPath(node: CreativeNode): string {
  if (node.content.kind !== "ink") return "";
  const c = node.content;
  return drawStrokeOutlinePath({
    id: node.id,
    color: c.color,
    input: c.input,
    brush: c.brush,
    points: c.points.map((p) => ({
      x: p.x,
      y: p.y,
      pressure: p.pressure ?? undefined,
      elapsedMs: p.elapsed_ms ?? undefined,
      tiltX: p.tilt_x ?? undefined,
      tiltY: p.tilt_y ?? undefined,
    })),
  });
}
export type CreativeStarter =
  "blank" | "screenshot" | "diagram" | "concept" | "composition";
export function starterScene(starter: CreativeStarter): CreativeScene {
  if (starter === "blank")
    return {
      schema: "medousa-create",
      version: 3,
      background: "transparent",
      nodes: [],
    };
  const size =
    starter === "composition"
      ? { width: 1024, height: 1024 }
      : { width: 1200, height: 920 };
  const frame = createNode(
    {
      kind: "frame",
      name:
        starter === "screenshot"
          ? "Screenshot markup"
          : starter === "diagram"
            ? "Diagram"
            : starter === "concept"
              ? "UI concept"
              : "Image composition",
      background: "#f7f5ee",
    },
    { x: 0, y: 0, ...size },
  );
  return {
    schema: "medousa-create",
    version: 3,
    background: "transparent",
    nodes: [frame],
  };
}

/** Keep a rotated object's top-left world anchor fixed while its local size changes. */
export function resizeNode(
  node: CreativeNode,
  width: number,
  height: number,
): CreativeNode {
  width = Math.max(1, width);
  height = Math.max(1, height);
  let content = node.content;
  if (content.kind === "ink")
    content = {
      ...content,
      points: content.points.map((p) => ({
        ...p,
        x: (p.x * width) / node.transform.width,
        y: (p.y * height) / node.transform.height,
      })),
    };
  const next = {
      ...node,
      content,
      transform: { ...node.transform, width, height },
    },
    before = localMatrix(node),
    after = localMatrix(next);
  next.transform.x += before[4] - after[4];
  next.transform.y += before[5] - after[5];
  return next;
}

/** Create's larger frames need a wider zoom range than legacy note ink. */
export function creativeZoom(
  camera: DrawCamera,
  anchor: DrawVector,
  requested: number,
): DrawCamera {
  const zoom = Math.max(0.02, Math.min(8, requested));
  if (zoom >= 0.25) return zoomDrawCameraAt(camera, anchor, zoom);
  const scene = viewToScene(camera, anchor);
  return {
    zoom,
    panX: anchor.x - scene.x * zoom,
    panY: anchor.y - scene.y * zoom,
  };
}
export function creativeFit(
  bounds: DrawRect | null,
  viewport: DrawSize,
  padding = Math.min(56, viewport.width * 0.1),
): DrawCamera {
  const fitted = fitDrawCamera(bounds, viewport, padding);
  if (!bounds) return fitted;
  return creativeZoom(
    fitted,
    { x: viewport.width / 2, y: viewport.height / 2 },
    Math.min(
      (viewport.width - padding * 2) / Math.max(1, bounds.width),
      (viewport.height - padding * 2) / Math.max(1, bounds.height),
    ),
  );
}
