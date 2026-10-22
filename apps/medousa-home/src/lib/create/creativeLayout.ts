import type { CreativeNode, CreativeOperation } from "$lib/types/generated/daemon_api";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { inverse, mapPoint, multiply, nodeDimensions, parentPoint, selectionRoots, worldMatrix } from "./creativeScene";
import { alignSelection, descendants, objectBounds, type Alignment } from "./creativeSelection";

export type LayoutPreferences = { grid: "none" | "dots" | "lines"; gridSize: number; snapGrid: boolean; smartGuides: boolean };
export const defaultLayout: LayoutPreferences = { grid: "none", gridSize: 16, snapGrid: false, smartGuides: true };
export type AlignmentTarget = "selection" | "frame";
export type LayoutGuide = { axis: "x" | "y"; value: number; from: number; to: number };
export function snapLayoutPoint(point: DrawVector, settings: LayoutPreferences, bypass = false): DrawVector {
  return settings.snapGrid && !bypass ? { x: Math.round(point.x / settings.gridSize) * settings.gridSize, y: Math.round(point.y / settings.gridSize) * settings.gridSize } : point;
}

function ancestors(node: CreativeNode, nodes: CreativeNode[]) {
  const result: CreativeNode[] = [];
  let parent = nodes.find(n => n.id === node.parent_id);
  while (parent) { result.push(parent); parent = nodes.find(n => n.id === parent!.parent_id); }
  return result;
}
export function containingFrame(ids: string[], nodes: CreativeNode[]) {
  const roots = selectionRoots(ids, nodes);
  if (!roots.length) return undefined;
  return ancestors(roots[0], nodes).find(n => n.content.kind === "frame" && roots.every(root => ancestors(root, nodes).some(a => a.id === n.id)));
}
function translated(node: CreativeNode, nodes: CreativeNode[], delta: DrawVector): CreativeOperation {
  const a = parentPoint(node, nodes, { x: 0, y: 0 }), b = parentPoint(node, nodes, delta);
  return { op: "put_node", node: { ...node, transform: { ...node.transform, x: node.transform.x + b.x - a.x, y: node.transform.y + b.y - a.y } } };
}
/** Measure actual corners in the frame's axes, including nested/rotated groups. */
function frameBounds(node: CreativeNode, nodes: CreativeNode[], frame: CreativeNode): DrawRect {
  const children = node.content.kind === "group" ? nodes.filter(n => n.parent_id === node.id && n.visible) : [];
  if (children.length) return union(children.map(n => frameBounds(n, nodes, frame)));
  const matrix = multiply(inverse(worldMatrix(frame, nodes)), worldMatrix(node, nodes)), { width, height } = nodeDimensions(node);
  const points = [{ x: 0, y: 0 }, { x: width, y: 0 }, { x: width, y: height }, { x: 0, y: height }].map(p => mapPoint(matrix, p));
  const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y));
  return { x, y, width: Math.max(...points.map(p => p.x)) - x, height: Math.max(...points.map(p => p.y)) - y };
}
export function alignObjects(ids: string[], nodes: CreativeNode[], alignment: Alignment, target: AlignmentTarget): CreativeOperation[] {
  if (target === "selection") return alignSelection(ids, nodes, alignment);
  const frame = containingFrame(ids, nodes);
  if (!frame) return [];
  const matrix = worldMatrix(frame, nodes), origin = mapPoint(matrix, { x: 0, y: 0 });
  return selectionRoots(ids, nodes).map(node => {
    const b = frameBounds(node, nodes, frame);
    const x = alignment === "left" ? -b.x : alignment === "center" ? (frame.transform.width - b.width) / 2 - b.x : alignment === "right" ? frame.transform.width - b.width - b.x : 0;
    const y = alignment === "top" ? -b.y : alignment === "middle" ? (frame.transform.height - b.height) / 2 - b.y : alignment === "bottom" ? frame.transform.height - b.height - b.y : 0;
    const p = mapPoint(matrix, { x, y });
    return translated(node, nodes, { x: p.x - origin.x, y: p.y - origin.y });
  });
}
export function distributeObjects(ids: string[], nodes: CreativeNode[], axis: "x" | "y"): CreativeOperation[] {
  const size = axis === "x" ? "width" : "height";
  const items = selectionRoots(ids, nodes).map(node => ({ node, box: objectBounds(node, nodes) })).sort((a, b) => a.box[axis] + a.box[size] / 2 - b.box[axis] - b.box[size] / 2);
  if (items.length < 3) return [];
  const first = items[0].box, last = items.at(-1)!.box;
  const gap = (last[axis] + last[size] - first[axis] - items.reduce((total, item) => total + item.box[size], 0)) / (items.length - 1);
  let cursor = first[axis] + first[size] + gap;
  return items.slice(1, -1).map(({ node, box }) => {
    const delta = cursor - box[axis]; cursor += box[size] + gap;
    return translated(node, nodes, { x: axis === "x" ? delta : 0, y: axis === "y" ? delta : 0 });
  });
}
function union(boxes: DrawRect[]): DrawRect {
  const x = Math.min(...boxes.map(b => b.x)), y = Math.min(...boxes.map(b => b.y));
  return { x, y, width: Math.max(...boxes.map(b => b.x + b.width)) - x, height: Math.max(...boxes.map(b => b.y + b.height)) - y };
}
export function layoutSnapTargets(ids: string[], nodes: CreativeNode[]): DrawRect[] {
  const roots = selectionRoots(ids, nodes), excluded = descendants(roots.map(n => n.id), nodes);
  const containers = new Set(roots.flatMap(n => ancestors(n, nodes).map(a => a.id)));
  const parents = new Set(roots.map(n => n.parent_id));
  return nodes.filter(n => !excluded.has(n.id) && (!containers.has(n.id) || n.content.kind === "frame") && (parents.has(n.parent_id) || containers.has(n.id)) && [n, ...ancestors(n, nodes)].every(a => a.visible && a.opacity > 0)).map(n => objectBounds(n, nodes));
}
/** Screen-space tolerance stays steady across zoom; guides win over grid on each axis. */
export function snapLayoutMove(bounds: DrawRect, targets: DrawRect[], delta: DrawVector, settings: LayoutPreferences, zoom: number, bypass = false) {
  const result = { delta: { ...delta }, guides: [] as LayoutGuide[] };
  if (bypass || (!delta.x && !delta.y)) return result;
  for (const axis of ["x", "y"] as const) {
    const size = axis === "x" ? "width" : "height", other = axis === "x" ? "y" : "x", otherSize = axis === "x" ? "height" : "width";
    const start = bounds[axis] + delta[axis], anchors = [start, start + bounds[size] / 2, start + bounds[size]];
    let closest: { offset: number; value: number; target: DrawRect } | undefined;
    if (settings.smartGuides) for (const target of targets) {
      for (const value of [target[axis], target[axis] + target[size] / 2, target[axis] + target[size]]) for (const anchor of anchors) {
        const offset = value - anchor;
        if (Math.abs(offset) <= 6 / zoom && (!closest || Math.abs(offset) < Math.abs(closest.offset))) closest = { offset, value, target };
      }
    }
    if (closest) {
      result.delta[axis] += closest.offset;
      result.guides.push({ axis, value: closest.value, from: Math.min(bounds[other] + delta[other], closest.target[other]), to: Math.max(bounds[other] + delta[other] + bounds[otherSize], closest.target[other] + closest.target[otherSize]) });
    } else if (settings.snapGrid) result.delta[axis] += Math.round(start / settings.gridSize) * settings.gridSize - start;
  }
  return result;
}
