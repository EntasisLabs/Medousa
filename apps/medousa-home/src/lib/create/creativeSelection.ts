import type { CreativeNode, CreativeOperation } from "$lib/types/generated/daemon_api";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { createNode, inverse, localMatrix, mapPoint, multiply, nodeBounds, parentPoint, resizeNode, selectionRoots, worldMatrix, type Matrix } from "./creativeScene";

export type SelectionEdit = { operations: CreativeOperation[]; selected: string[] };
export function descendants(ids: string[], nodes: CreativeNode[]): Set<string> {
  const children = new Map<string, string[]>();
  for (const n of nodes) if (n.parent_id) children.set(n.parent_id, [...(children.get(n.parent_id) ?? []), n.id]);
  const result = new Set(ids), queue = [...ids];
  while (queue.length) for (const id of children.get(queue.pop()!) ?? []) if (!result.has(id)) { result.add(id); queue.push(id); }
  return result;
}
function union(rects: DrawRect[]): DrawRect | null {
  if (!rects.length) return null;
  const x = Math.min(...rects.map(r => r.x)), y = Math.min(...rects.map(r => r.y));
  return { x, y, width: Math.max(1, ...rects.map(r => r.x + r.width - x)), height: Math.max(1, ...rects.map(r => r.y + r.height - y)) };
}
export function objectBounds(node: CreativeNode, nodes: CreativeNode[]): DrawRect {
  if (node.content.kind !== "group") return nodeBounds(node, nodes);
  const children = nodes.filter(n => n.parent_id === node.id && n.visible);
  return union(children.map(n => objectBounds(n, nodes))) ?? nodeBounds(node, nodes);
}
export function selectionBounds(ids: string[], nodes: CreativeNode[]): DrawRect | null {
  return union(selectionRoots(ids, nodes).map(n => objectBounds(n, nodes)));
}
export function canGroup(ids: string[], nodes: CreativeNode[]): boolean {
  const roots = selectionRoots(ids, nodes);
  return roots.length >= 2 && roots.length <= 998 && nodes.length < 10000 && roots.every(n => (n.parent_id ?? null) === (roots[0].parent_id ?? null));
}
function atMatrix(node: CreativeNode, matrix: Matrix, parent: CreativeNode["parent_id"]): CreativeNode {
  const rotation = Math.atan2(matrix[1], matrix[0]) * 180 / Math.PI;
  const next = { ...node, parent_id: parent ?? null, transform: { ...node.transform, x: 0, y: 0, rotation } };
  const origin = localMatrix(next);
  next.transform.x = matrix[4] - origin[4]; next.transform.y = matrix[5] - origin[5];
  return next;
}
function checkSize(operations: CreativeOperation[]) {
  if (operations.length > 1000) throw new Error("Select fewer objects for this edit (up to 1,000 operations).");
}
export function groupSelection(ids: string[], nodes: CreativeNode[]): SelectionEdit {
  if (!canGroup(ids, nodes)) throw new Error("Select at least two objects in the same frame or group to group them.");
  const roots = selectionRoots(ids, nodes), parent = roots[0].parent_id;
  const rects = roots.map(n => {
    const m = localMatrix(n), w = n.transform.width, h = n.transform.height;
    const points = [{ x: 0, y: 0 }, { x: w, y: 0 }, { x: w, y: h }, { x: 0, y: h }].map(p => mapPoint(m, p));
    const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y));
    return { x, y, width: Math.max(...points.map(p => p.x)) - x, height: Math.max(...points.map(p => p.y)) - y };
  });
  const group = { ...createNode({ kind: "group" }, union(rects)!, parent), name: `Group ${nodes.filter(n => n.content.kind === "group").length + 1}` };
  const groupInverse = inverse(localMatrix(group));
  const last = nodes.lastIndexOf(roots[roots.length - 1]);
  const order = nodes.flatMap((n, i) => i === last ? [group.id, n.id] : [n.id]);
  const operations: CreativeOperation[] = [
    { op: "put_node", node: group },
    ...roots.map(node => ({ op: "put_node" as const, node: atMatrix(node, multiply(groupInverse, localMatrix(node)), group.id) })),
    { op: "reorder", ids: order },
  ];
  return { operations, selected: [group.id] };
}
export function ungroupSelection(ids: string[], nodes: CreativeNode[]): SelectionEdit {
  const groups = selectionRoots(ids, nodes).filter(n => n.content.kind === "group");
  if (!groups.length) return { operations: [], selected: ids };
  if (groups.some(n => n.opacity !== 1)) throw new Error("Set group opacity to 100% before ungrouping to preserve overlapping artwork.");
  const operations: CreativeOperation[] = [], selected = selectionRoots(ids, nodes).filter(n => n.content.kind !== "group").map(n => n.id);
  const replacements = new Map<string, string[]>(), lifted = new Set<string>();
  for (const group of groups) {
    const children = nodes.filter(n => n.parent_id === group.id);
    replacements.set(group.id, children.map(n => n.id));
    for (const child of children) {
      lifted.add(child.id); selected.push(child.id);
      operations.push({ op: "put_node", node: {
        ...atMatrix(child, multiply(localMatrix(group), localMatrix(child)), group.parent_id),
      } });
    }
    operations.push({ op: "remove_node", id: group.id });
  }
  operations.push({ op: "reorder", ids: nodes.flatMap(n => replacements.get(n.id) ?? (lifted.has(n.id) ? [] : [n.id])) });
  checkSize(operations);
  return { operations, selected };
}
/** Groups are picked as objects until entered; explicit layer picks remain editable. */
export function selectionTarget(id: string, nodes: CreativeNode[], scope: string | null, selected: string[] = []): string {
  if (selected.includes(id)) return id;
  let node = nodes.find(n => n.id === id), target = id;
  while (node?.parent_id && node.parent_id !== scope) {
    node = nodes.find(n => n.id === node!.parent_id);
    if (node?.content.kind === "group") target = node.id;
  }
  return target;
}
/** Uniform scaling bakes sizes into descendants: no new renderer matrix or shear. */
export function scaleSelection(ids: string[], nodes: CreativeNode[], anchor: DrawVector, factor: number): CreativeNode[] {
  const roots = selectionRoots(ids, nodes), affected = descendants(roots.map(n => n.id), nodes);
  let min = .01, max = 100;
  for (const node of nodes) if (affected.has(node.id)) {
    if (node.transform.width > 0) max = Math.min(max, 1e7 / node.transform.width);
    if (node.transform.height > 0) max = Math.min(max, 1e7 / node.transform.height);
    const c = node.content;
    if (c.kind === "ink") { min = Math.max(min, .25 / c.brush.size); max = Math.min(max, 160 / c.brush.size); }
    if (c.kind === "shape" && c.stroke_width > 0) max = Math.min(max, 1000 / c.stroke_width);
    if (c.kind === "text") { min = Math.max(min, 4 / c.font_size); max = Math.min(max, 512 / c.font_size); }
  }
  factor = Math.max(min, Math.min(max, factor));
  const worlds = new Map(nodes.map(n => [n.id, worldMatrix(n, nodes)]));
  const scaled = new Map<string, Matrix>();
  for (const id of affected) {
    const m = worlds.get(id)!;
    scaled.set(id, [m[0], m[1], m[2], m[3], anchor.x + (m[4] - anchor.x) * factor, anchor.y + (m[5] - anchor.y) * factor]);
  }
  return nodes.map(n => {
    if (!affected.has(n.id)) return n;
    const parent = n.parent_id ? scaled.get(n.parent_id) ?? worlds.get(n.parent_id)! : [1, 0, 0, 1, 0, 0] as Matrix;
    let next = resizeNode(n, n.transform.width * factor, n.transform.height * factor);
    const c = next.content;
    if (c.kind === "ink") next = { ...next, content: { ...c, brush: { ...c.brush, size: Math.max(.25, Math.min(160, c.brush.size * factor)) } } };
    if (c.kind === "text") next = { ...next, content: { ...c, font_size: Math.max(4, Math.min(512, c.font_size * factor)) } };
    if (c.kind === "shape") next = { ...next, content: { ...c, stroke_width: c.stroke_width * factor } };
    return atMatrix(next, multiply(inverse(parent), scaled.get(n.id)!), n.parent_id);
  });
}
export type Alignment = "left" | "center" | "right" | "top" | "middle" | "bottom";
export function alignSelection(ids: string[], nodes: CreativeNode[], alignment: Alignment): CreativeOperation[] {
  const roots = selectionRoots(ids, nodes), all = selectionBounds(ids, nodes);
  if (!all || roots.length < 2) return [];
  return roots.map(n => {
    const b = objectBounds(n, nodes);
    const dx = alignment === "left" ? all.x - b.x : alignment === "center" ? all.x + all.width / 2 - b.x - b.width / 2 : alignment === "right" ? all.x + all.width - b.x - b.width : 0;
    const dy = alignment === "top" ? all.y - b.y : alignment === "middle" ? all.y + all.height / 2 - b.y - b.height / 2 : alignment === "bottom" ? all.y + all.height - b.y - b.height : 0;
    const a = parentPoint(n, nodes, { x: 0, y: 0 }), p = parentPoint(n, nodes, { x: dx, y: dy });
    return { op: "put_node" as const, node: { ...n, transform: { ...n.transform, x: n.transform.x + p.x - a.x, y: n.transform.y + p.y - a.y } } };
  });
}
export function arrangeSelection(ids: string[], nodes: CreativeNode[], direction: -1 | 1): CreativeOperation[] {
  const roots = selectionRoots(ids, nodes), selected = new Set(roots.map(n => n.id));
  const order = nodes.map(n => n.id);
  for (const parent of new Set(roots.map(n => n.parent_id))) {
    const siblings = nodes.filter(n => n.parent_id === parent).map(n => n.id);
    const indices = siblings.map(id => order.indexOf(id));
    const steps = Array.from({ length: siblings.length }, (_, i) => i);
    if (direction === 1) steps.reverse();
    for (const i of steps) {
      const j = i + direction;
      if (j >= 0 && j < siblings.length && selected.has(siblings[i]) && !selected.has(siblings[j])) [siblings[i], siblings[j]] = [siblings[j], siblings[i]];
    }
    indices.forEach((index, i) => { order[index] = siblings[i]; });
  }
  return order.some((id, i) => id !== nodes[i].id) ? [{ op: "reorder", ids: order }] : [];
}
export type LayerRow = { node: CreativeNode; depth: number; children: number };
export function layerRows(nodes: CreativeNode[], collapsed: (n: CreativeNode) => boolean): LayerRow[] {
  const children = new Map<string | null, CreativeNode[]>();
  for (const node of nodes) children.set(node.parent_id ?? null, [...(children.get(node.parent_id ?? null) ?? []), node]);
  const stack = [...(children.get(null) ?? [])].map(node => ({ node, depth: 0 })), rows: LayerRow[] = [];
  while (stack.length) {
    const row = stack.pop()!, nested = children.get(row.node.id) ?? [];
    rows.push({ ...row, children: nested.length });
    if (!collapsed(row.node)) for (const node of nested) stack.push({ node, depth: row.depth + 1 });
  }
  return rows;
}

export function resizeSelectionFromHandle(ids: string[], nodes: CreativeNode[], handle: string, point: DrawVector, preserveAspect = false): CreativeNode[] {
  const roots = selectionRoots(ids, nodes), bounds = selectionBounds(ids, nodes);
  if (!bounds || !roots.length) return nodes;
  if (roots.length > 1 || roots[0].content.kind === "group") {
    const anchor = { x: handle.includes("w") ? bounds.x + bounds.width : bounds.x, y: handle.includes("n") ? bounds.y + bounds.height : bounds.y };
    const rx = (point.x - anchor.x) / (handle.includes("w") ? -bounds.width : bounds.width);
    const ry = (point.y - anchor.y) / (handle.includes("n") ? -bounds.height : bounds.height);
    const factor = Math.abs(rx - 1) > Math.abs(ry - 1) ? rx : ry;
    return scaleSelection(ids, nodes, anchor, factor);
  }
  const node = roots[0], { width: w, height: h } = node.transform;
  const q = mapPoint(inverse(worldMatrix(node, nodes)), point);
  let x = handle.includes("w") ? Math.min(w - 4, q.x) : 0;
  let y = handle.includes("n") ? Math.min(h - 4, q.y) : 0;
  let width = handle.includes("w") ? w - x : handle.includes("e") ? Math.max(4, q.x) : w;
  let height = handle.includes("n") ? h - y : handle.includes("s") ? Math.max(4, q.y) : h;
  if (preserveAspect) {
    const rx = width / Math.max(1, w), ry = height / Math.max(1, h);
    const factor = Math.max(.01, Math.abs(rx - 1) > Math.abs(ry - 1) ? rx : ry);
    width = Math.max(4, w * factor); height = Math.max(4, h * factor);
    x = handle.includes("w") ? w - width : handle === "n" || handle === "s" ? (w - width) / 2 : 0;
    y = handle.includes("n") ? h - height : handle === "e" || handle === "w" ? (h - height) / 2 : 0;
  }
  const next = resizeNode(node, width, height);
  const target = mapPoint(localMatrix(node), { x, y }), origin = localMatrix(next);
  next.transform.x += target.x - origin[4]; next.transform.y += target.y - origin[5];
  return nodes.map(n => n.id === node.id ? next : n);
}
