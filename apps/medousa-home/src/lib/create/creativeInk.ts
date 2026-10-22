import type {
  CreativeNode,
  CreativeScene,
  CreativeOperation,
} from "$lib/types/generated/daemon_api";
import type { DrawStroke } from "$lib/draw/drawDocument";
import type { DrawVector } from "$lib/draw/drawCamera";
import {
  eraseDrawStrokeByPath,
  hitTestDrawStroke,
  selectDrawStrokesInLasso,
} from "$lib/draw/drawGeometry";
import {
  createNode,
  editableNode,
  inverse,
  mapPoint,
  nodeBounds,
  parentPoint,
  selectionRoots,
  worldMatrix,
} from "./creativeScene";
import { layerRows } from "./creativeSelection";
import { hitCanvasNode } from "./creativeInteraction";
import { randomUuid } from "$lib/utils/randomUuid";

export function creativeStroke(
  node: CreativeNode,
  nodes: CreativeNode[],
): DrawStroke | null {
  if (node.content.kind !== "ink") return null;
  const c = node.content,
    m = worldMatrix(node, nodes);
  return {
    id: node.id,
    color: c.color,
    input: c.input,
    brush: c.brush,
    points: c.points.map((p) => ({
      ...mapPoint(m, p),
      pressure: p.pressure ?? undefined,
      elapsedMs: p.elapsed_ms ?? undefined,
      tiltX: p.tilt_x ?? undefined,
      tiltY: p.tilt_y ?? undefined,
    })),
  };
}
/** Normalize back into the same parent space; stroke fragments retain appearance
 * and metadata while receiving IDs accepted by the existing daemon contract. */
export function creativeStrokeNode(
  stroke: DrawStroke,
  nodes: CreativeNode[],
  parent: CreativeNode | null,
  original?: CreativeNode,
): CreativeNode {
  const m = parent ? inverse(worldMatrix(parent, nodes)) : null;
  const points = stroke.points.map((p) => ({
    ...p,
    ...(m ? mapPoint(m, p) : p),
  }));
  const x = Math.min(...points.map((p) => p.x)),
    y = Math.min(...points.map((p) => p.y));
  const content: CreativeNode["content"] = {
    kind: "ink",
    color: stroke.color,
    input: stroke.input,
    brush: { ...stroke.brush },
    points: points.map((p) => ({
      x: p.x - x,
      y: p.y - y,
      pressure: p.pressure,
      elapsed_ms: p.elapsedMs,
      tilt_x: p.tiltX,
      tilt_y: p.tiltY,
    })),
  };
  const node = createNode(
    content,
    {
      x,
      y,
      width: Math.max(1, Math.max(...points.map((p) => p.x)) - x),
      height: Math.max(1, Math.max(...points.map((p) => p.y)) - y),
    },
    parent?.id ?? null,
  );
  return {
    ...(original ?? node),
    id: original?.id ?? stroke.id,
    transform: node.transform,
    content,
  };
}
export function eraseCreativeInk(
  scene: CreativeScene,
  path: DrawVector[],
  radius: number,
  mode: "partial" | "stroke",
): CreativeScene {
  let changed = false;
  const nodes = scene.nodes.flatMap((node) => {
    if (!editableNode(node, scene.nodes)) return [node];
    const stroke = creativeStroke(node, scene.nodes);
    if (!stroke) {
      if (
        mode === "stroke" &&
        path.some((p) => hitCanvasNode(node, scene.nodes, p, radius))
      ) {
        changed = true;
        return [];
      }
      return [node];
    }
    if (mode === "stroke") {
      if (!path.some((p) => hitTestDrawStroke(stroke, p, radius)))
        return [node];
      changed = true;
      return [];
    }
    const parts = eraseDrawStrokeByPath(stroke, path, radius);
    if (parts.length === 1 && parts[0] === stroke) return [node];
    changed = true;
    const parent = scene.nodes.find((n) => n.id === node.parent_id) ?? null;
    return parts.map((part, i) => {
      const id = i ? randomUuid() : node.id;
      return creativeStrokeNode({ ...part, id }, scene.nodes, parent, {
        ...node,
        id,
      });
    });
  });
  return changed ? { ...scene, nodes } : scene;
}
export function hitCreativeScene(
  scene: CreativeScene,
  point: DrawVector,
  padding: number,
): string | null {
  for (const { node: n } of layerRows(scene.nodes, () => false)) {
    if (
      !editableNode(n, scene.nodes) ||
      n.content.kind === "frame" ||
      n.content.kind === "group"
    )
      continue;
    const stroke = creativeStroke(n, scene.nodes);
    if (
      stroke
        ? hitTestDrawStroke(stroke, point, padding)
        : hitCanvasNode(n, scene.nodes, point, padding)
    )
      return n.id;
  }
  return null;
}
export function lassoCreativeScene(
  scene: CreativeScene,
  path: DrawVector[],
): string[] {
  if (path.length < 3) return [];
  const ids = scene.nodes.flatMap((n) => {
    if (
      !editableNode(n, scene.nodes) ||
      n.content.kind === "frame" ||
      n.content.kind === "group"
    )
      return [];
    const stroke = creativeStroke(n, scene.nodes);
    if (stroke) return selectDrawStrokesInLasso([stroke], path);
    const b = nodeBounds(n, scene.nodes);
    const outline: DrawStroke = {
      id: n.id,
      color: "#000",
      input: "mouse",
      brush: {
        kind: "pen",
        size: 1,
        thinning: 0,
        smoothing: 0,
        streamline: 0,
        opacity: 1,
      },
      points: [
        { x: b.x, y: b.y },
        { x: b.x + b.width, y: b.y },
        { x: b.x + b.width, y: b.y + b.height },
        { x: b.x, y: b.y + b.height },
        { x: b.x, y: b.y },
      ],
    };
    return selectDrawStrokesInLasso([outline], path);
  });
  return selectionRoots(ids, scene.nodes).map((n) => n.id);
}
export function moveCreativeSelection(
  scene: CreativeScene,
  ids: string[],
  delta: DrawVector,
): CreativeScene {
  const roots = selectionRoots(ids, scene.nodes);
  if (!delta.x && !delta.y) return scene;
  return {
    ...scene,
    nodes: scene.nodes.map((n) => {
      if (!roots.includes(n)) return n;
      const a = parentPoint(n, scene.nodes, { x: 0, y: 0 }),
        b = parentPoint(n, scene.nodes, delta);
      return {
        ...n,
        transform: {
          ...n.transform,
          x: n.transform.x + b.x - a.x,
          y: n.transform.y + b.y - a.y,
        },
      };
    }),
  };
}
export function creativeSceneChanges(
  before: CreativeScene,
  after: CreativeScene,
): CreativeOperation[] {
  if (before === after) return [];
  const previous = new Map(before.nodes.map((n) => [n.id, n]));
  const next = new Set(after.nodes.map((n) => n.id));
  return [
    ...before.nodes
      .filter((n) => !next.has(n.id))
      .map((n) => ({ op: "remove_node" as const, id: n.id })),
    ...after.nodes
      .filter(
        (n) =>
          previous.get(n.id) !== n &&
          JSON.stringify(previous.get(n.id)) !== JSON.stringify(n),
      )
      .map((node) => ({ op: "put_node" as const, node })),
  ];
}
