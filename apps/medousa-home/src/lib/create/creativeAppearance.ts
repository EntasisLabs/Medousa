import type { CreativeNode, CreativeShadow } from "$lib/types/generated/daemon_api";
import type { DrawRect, DrawVector } from "$lib/draw/drawCamera";
import { inverse, mapPoint, nodeBounds, worldMatrix } from "./creativeScene";

export type AppearancePatch = { corner_radius?: number; shadow?: CreativeShadow | null; fill_color?: string; stroke_color?: string; border_width?: number };
export const defaultShadow: CreativeShadow = { x: 0, y: 6, blur: 12, color: "#000000", opacity: .18 };
export function hasSurface(node: CreativeNode): boolean {
  const c = node.content;
  return c.kind === "frame" || c.kind === "image" || c.kind === "shape" && c.shape !== "line";
}
export function hasCorners(node: CreativeNode): boolean {
  return hasSurface(node) && !(node.content.kind === "shape" && node.content.shape === "ellipse");
}
export function cornerRadius(node: CreativeNode): number {
  return hasCorners(node) ? Math.max(0, Math.min(node.appearance?.corner_radius ?? 0, node.transform.width / 2, node.transform.height / 2)) : 0;
}
export function styleAppearance(node: CreativeNode, patch: AppearancePatch): CreativeNode {
  if (!hasSurface(node)) return node;
  let next = node;
  if (patch.corner_radius !== undefined || patch.shadow !== undefined) next = { ...next, appearance: {
    corner_radius: patch.corner_radius !== undefined && Number.isFinite(patch.corner_radius) ? Math.max(0, Math.min(1e7, patch.corner_radius)) : next.appearance?.corner_radius ?? 0,
    shadow: patch.shadow !== undefined ? patch.shadow : next.appearance?.shadow ?? null,
  } };
  const c = next.content;
  if (c.kind === "shape") next = { ...next, content: { ...c,
    fill: patch.fill_color ?? c.fill, stroke: patch.stroke_color ?? c.stroke,
    stroke_width: patch.border_width !== undefined && Number.isFinite(patch.border_width) ? Math.max(0, Math.min(1000, patch.border_width)) : c.stroke_width,
  } };
  if (c.kind === "frame" && patch.fill_color) next = { ...next, content: { ...c, background: patch.fill_color } };
  return next;
}
export function dragCornerRadius(node: CreativeNode, nodes: CreativeNode[], point: DrawVector): CreativeNode {
  const p = mapPoint(inverse(worldMatrix(node, nodes)), point);
  return styleAppearance(node, { corner_radius: Math.max(0, Math.min(node.transform.width / 2, node.transform.height / 2, (p.x + p.y) / 2)) });
}
/** Shadow/stroke extents affect exported pixels, never object selection or snapping. */
export function visualBounds(node: CreativeNode, nodes: CreativeNode[]): DrawRect {
  const bounds = nodeBounds(node, nodes), shadow = node.appearance?.shadow;
  const border = node.content.kind === "shape" ? node.content.stroke_width / 2 : 0;
  let pad = border;
  if (shadow) pad += shadow.blur * 4 + Math.hypot(shadow.x, shadow.y);
  if (node.content.kind === "group") {
    const children = nodes.filter(n => n.visible && n.parent_id === node.id).map(n => visualBounds(n, nodes));
    return unionVisualBounds(children) ?? bounds;
  }
  return { x: bounds.x - pad, y: bounds.y - pad, width: bounds.width + pad * 2, height: bounds.height + pad * 2 };
}
export function unionVisualBounds(bounds: { x: number; y: number; width: number; height: number }[]) {
  if (!bounds.length) return null;
  const x = Math.min(...bounds.map(b => b.x)), y = Math.min(...bounds.map(b => b.y));
  return { x, y, width: Math.max(...bounds.map(b => b.x + b.width)) - x, height: Math.max(...bounds.map(b => b.y + b.height)) - y };
}

export function roundedDistance(node: CreativeNode, p: DrawVector): number {
  const { width: w, height: h } = node.transform, r = cornerRadius(node);
  const dx = Math.abs(p.x - w / 2) - w / 2 + r, dy = Math.abs(p.y - h / 2) - h / 2 + r;
  return Math.hypot(Math.max(dx, 0), Math.max(dy, 0)) + Math.min(Math.max(dx, dy), 0) - r;
}
