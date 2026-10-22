import type { CreativeNode, CreativeOperation } from "$lib/types/generated/daemon_api";
import { lineGeometry, lineOutline, snapLinePoint, setLineEndpoints } from "./creativeLine";
import { styleNode } from "./creativeInteraction";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { createNode, inverse, localMatrix, mapPoint, multiply, nodeBounds, nodeDimensions, parentPoint, resizeNode, selectionRoots, worldMatrix, type Matrix } from "./creativeScene";

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
    const m = localMatrix(n), { width: w, height: h } = nodeDimensions(n);
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
    if (next.appearance) next = { ...next, appearance: { corner_radius: Math.min(1e7, next.appearance.corner_radius * factor), shadow: next.appearance.shadow ? { ...next.appearance.shadow, x: Math.max(-1000, Math.min(1000, next.appearance.shadow.x * factor)), y: Math.max(-1000, Math.min(1000, next.appearance.shadow.y * factor)), blur: Math.min(128, next.appearance.shadow.blur * factor) } : null } };
    if (next.text_layout?.width) next = { ...next, text_layout: { ...next.text_layout, width: Math.min(1e7, next.text_layout.width * factor) } };
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
    const frame = selectionFrame(ids, nodes)!;
    const pointLocal = mapPoint(inverse(frame.matrix), point);
    const anchor = mapPoint(frame.matrix, { x: handle.includes("w") ? frame.width : 0, y: handle.includes("n") ? frame.height : 0 });
    const rx = handle.includes("w") ? (frame.width - pointLocal.x) / frame.width : pointLocal.x / frame.width;
    const ry = handle.includes("n") ? (frame.height - pointLocal.y) / frame.height : pointLocal.y / frame.height;
    const factor = Math.abs(rx - 1) > Math.abs(ry - 1) ? rx : ry;
    return scaleSelection(ids, nodes, anchor, factor);
  }
  const node = roots[0], { width: w, height: h } = nodeDimensions(node);
  const q = mapPoint(inverse(worldMatrix(node, nodes)), point);
  let x = handle.includes("w") ? Math.min(w - 4, q.x) : 0;
  let y = handle.includes("n") ? Math.min(h - 4, q.y) : 0;
  let width = handle.includes("w") ? w - x : handle.includes("e") ? Math.max(4, q.x) : w;
  let height = handle.includes("n") ? h - y : handle.includes("s") ? Math.max(4, q.y) : h;
  const wrapText = node.content.kind === "text" && (handle === "e" || handle === "w") && !preserveAspect;
  if (preserveAspect || node.content.kind === "text" && !wrapText) {
    const rx = width / Math.max(1, w), ry = height / Math.max(1, h);
    const factor = Math.max(.01, Math.abs(rx - 1) > Math.abs(ry - 1) ? rx : ry);
    width = Math.max(4, w * factor); height = Math.max(4, h * factor);
    x = handle.includes("w") ? w - width : handle === "n" || handle === "s" ? (w - width) / 2 : 0;
    y = handle.includes("n") ? h - height : handle === "e" || handle === "w" ? (h - height) / 2 : 0;
  }
  let next = resizeNode(node, width, height);
  if (node.content.kind === "text") {
    next = styleNode(node, wrapText ? { text_width: width } : { font_size: node.content.font_size * width / w, ...(node.text_layout?.width ? { text_width: node.text_layout.width * width / w } : {}) });
    const fitted = nodeDimensions(next);
    x = handle.includes("w") ? w - fitted.width : 0;
    y = handle.includes("n") ? h - fitted.height : 0;
  }
  const target = mapPoint(localMatrix(node), { x, y }), origin = localMatrix(next);
  next.transform.x += target.x - origin[4]; next.transform.y += target.y - origin[5];
  return nodes.map(n => n.id === node.id ? next : n);
}

export type SelectionFrame = { matrix: Matrix; width: number; height: number; center: DrawVector; angle: number };
export function selectionFrame(ids: string[], nodes: CreativeNode[]): SelectionFrame | null {
  const roots = selectionRoots(ids, nodes), bounds = selectionBounds(ids, nodes);
  if (!roots.length || !bounds) return null;
  let matrix: Matrix = [1, 0, 0, 1, bounds.x, bounds.y], width = bounds.width, height = bounds.height;
  if (roots.length === 1) {
    const node = roots[0]; matrix = worldMatrix(node, nodes);
    if (node.content.kind === "group") {
      const points: DrawVector[] = [], toLocal = inverse(matrix), queue = [node.id];
      while (queue.length) {
        const parentId = queue.pop();
        for (const child of nodes.filter(n => n.parent_id === parentId)) {
        if (!child.visible) continue;
        if (child.content.kind === "group") { queue.push(child.id); continue; }
        const m = multiply(toLocal, worldMatrix(child, nodes)), line = lineGeometry(child), size = nodeDimensions(child);
        const outline = line ? lineOutline(line) : [{ x: 0, y: 0 }, { x: size.width, y: 0 }, { x: size.width, y: size.height }, { x: 0, y: size.height }];
        points.push(...outline.map(p => mapPoint(m, p)));
        }
      }
      if (points.length) {
        const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y));
        width = Math.max(1, ...points.map(p => p.x - x)); height = Math.max(1, ...points.map(p => p.y - y));
        matrix = multiply(matrix, [1, 0, 0, 1, x, y]);
      } else { width = node.transform.width; height = node.transform.height; }
    } else {
      const line = lineGeometry(node);
      if (line) {
        const start = mapPoint(matrix, line.start), end = mapPoint(matrix, line.end), angle = Math.atan2(end.y - start.y, end.x - start.x);
        const c = Math.cos(angle), s = Math.sin(angle);
        width = Math.hypot(end.x - start.x, end.y - start.y); height = 0;
        matrix = [c, s, -s, c, start.x, start.y];
      } else { const size = nodeDimensions(node); width = size.width; height = size.height; }
    }
  }
  return { matrix, width, height, center: mapPoint(matrix, { x: width / 2, y: height / 2 }), angle: Math.atan2(matrix[1], matrix[0]) * 180 / Math.PI };
}
function rotationMatrix(center: DrawVector, degrees: number): Matrix {
  const a = degrees * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
  return [c, s, -s, c, center.x - c * center.x + s * center.y, center.y - s * center.x - c * center.y];
}
/** Keep the multi-selection box attached to the rotation gesture, rather than its changing AABB. */
export function rotateSelectionFrame(frame: SelectionFrame, degrees: number): SelectionFrame {
  return { ...frame, matrix: multiply(rotationMatrix(frame.center, degrees), frame.matrix), angle: frame.angle + degrees };
}
/** Rotate selected roots in world space; children inherit the change exactly once. */
export function rotateSelection(ids: string[], nodes: CreativeNode[], center: DrawVector, degrees: number): CreativeNode[] {
  const roots = new Set(selectionRoots(ids, nodes).map(n => n.id)), rotation = rotationMatrix(center, degrees);
  return nodes.map(node => {
    if (!roots.has(node.id)) return node;
    const parent = nodes.find(n => n.id === node.parent_id), parentWorld: Matrix = parent ? worldMatrix(parent, nodes) : [1, 0, 0, 1, 0, 0];
    return atMatrix(node, multiply(inverse(parentWorld), multiply(rotation, worldMatrix(node, nodes))), node.parent_id);
  });
}
export function moveLineEndpoint(node: CreativeNode, nodes: CreativeNode[], endpoint: "start" | "end", point: DrawVector, snap: boolean): CreativeNode {
  const line = lineGeometry(node);
  if (!line) return node;
  const m = worldMatrix(node, nodes), fixed = mapPoint(m, endpoint === "start" ? line.end : line.start), next = snapLinePoint(fixed, point, snap);
  return setLineEndpoints(node, parentPoint(node, nodes, endpoint === "start" ? next : fixed), parentPoint(node, nodes, endpoint === "end" ? next : fixed));
}
