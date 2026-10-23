import type { CreativeNode } from "$lib/types/generated/daemon_api";
import type { DrawVector } from "$lib/draw/drawCamera";

export type LineStyle = { stroke_width: number; dashed: boolean; start_arrow: boolean; end_arrow: boolean };
export type LineStylePatch = Partial<LineStyle>;
export type LineGeometry = LineStyle & { start: DrawVector; end: DrawVector; color: string; opacity: number };

/** Recognize only the exact five-point arrow pattern emitted by the old tool. */
export function lineGeometry(node: CreativeNode): LineGeometry | null {
  const c = node.content;
  if (c.kind === "shape" && c.shape === "line") return {
    start: c.line?.start ?? { x: 0, y: 0 }, end: c.line?.end ?? { x: node.transform.width, y: node.transform.height },
    stroke_width: c.stroke_width, color: c.stroke, opacity: 1,
    start_arrow: c.line?.start_arrow ?? false, end_arrow: c.line?.end_arrow ?? false, dashed: c.line?.dashed ?? false,
  };
  if (c.kind !== "ink" || c.input !== "mouse" || c.brush.kind !== "pen" || c.points.length !== 5) return null;
  const [a, b, left, repeat, right] = c.points;
  if (b.x !== repeat.x || b.y !== repeat.y || c.points.some(p => p.pressure !== .5 || p.elapsed_ms != null || p.tilt_x != null || p.tilt_y != null)) return null;
  const distance = Math.hypot(b.x - a.x, b.y - a.y), l = Math.hypot(left.x - b.x, left.y - b.y), r = Math.hypot(right.x - b.x, right.y - b.y);
  if (distance < 1 || l < 1 || Math.abs(l - r) > .01 || l > distance * .36) return null;
  const direction = Math.atan2(b.y - a.y, b.x - a.x);
  if (Math.abs((b.x - left.x) / l - Math.cos(direction - .5)) > .001 || Math.abs((b.y - left.y) / l - Math.sin(direction - .5)) > .001 || Math.abs((b.x - right.x) / r - Math.cos(direction + .5)) > .001 || Math.abs((b.y - right.y) / r - Math.sin(direction + .5)) > .001) return null;
  return { start: a, end: b, stroke_width: c.brush.size, color: c.color, opacity: c.brush.opacity, start_arrow: false, end_arrow: true, dashed: false };
}

/** Snap relative to the fixed endpoint, in scene space (also inside rotated frames). */
export function snapLinePoint(anchor: DrawVector, point: DrawVector, snap: boolean): DrawVector {
  if (!snap) return point;
  const dx = point.x - anchor.x, dy = point.y - anchor.y;
  const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * Math.PI / 4, length = Math.hypot(dx, dy);
  return { x: anchor.x + Math.cos(angle) * length, y: anchor.y + Math.sin(angle) * length };
}

export function setLineEndpoints(node: CreativeNode, start: DrawVector, end: DrawVector, patch: LineStylePatch = {}): CreativeNode {
  const previous = lineGeometry(node);
  const x = Math.min(start.x, end.x), y = Math.min(start.y, end.y);
  return {
    ...node, opacity: node.opacity * (previous?.opacity ?? 1),
    transform: { x, y, width: Math.max(1, Math.abs(end.x - start.x)), height: Math.max(1, Math.abs(end.y - start.y)), rotation: 0 },
    content: { kind: "shape", shape: "line", fill: "transparent", stroke: previous?.color ?? "#393e33", stroke_width: patch.stroke_width ?? previous?.stroke_width ?? 4,
      line: { start: { x: start.x - x, y: start.y - y }, end: { x: end.x - x, y: end.y - y },
        start_arrow: patch.start_arrow ?? previous?.start_arrow ?? false, end_arrow: patch.end_arrow ?? previous?.end_arrow ?? false, dashed: patch.dashed ?? previous?.dashed ?? false },
    },
  };
}

export function linePaint(line: LineGeometry) {
  const { start: a, end: b } = line, length = Math.hypot(b.x - a.x, b.y - a.y);
  const ux = length ? (b.x - a.x) / length : 1, uy = length ? (b.y - a.y) / length : 0;
  const head = Math.min(length * (line.start_arrow && line.end_arrow ? .4 : .6), line.stroke_width * 4);
  const triangle = (tip: DrawVector, direction: number) => [tip,
    { x: tip.x + direction * ux * head - uy * head * .45, y: tip.y + direction * uy * head + ux * head * .45 },
    { x: tip.x + direction * ux * head + uy * head * .45, y: tip.y + direction * uy * head - ux * head * .45 }];
  return {
    start: { x: a.x + ux * (line.start_arrow ? head * .8 : 0), y: a.y + uy * (line.start_arrow ? head * .8 : 0) },
    end: { x: b.x - ux * (line.end_arrow ? head * .8 : 0), y: b.y - uy * (line.end_arrow ? head * .8 : 0) },
    heads: [...(line.start_arrow && head > .001 ? [triangle(a, 1)] : []), ...(line.end_arrow && head > .001 ? [triangle(b, -1)] : [])],
  };
}
export function lineOutline(line: LineGeometry): DrawVector[] {
  const points = [line.start, line.end, ...linePaint(line).heads.flat()], padding = line.stroke_width / 2;
  return points.flatMap(p => [{ x: p.x - padding, y: p.y - padding }, { x: p.x + padding, y: p.y + padding }]);
}
export function hitLine(line: LineGeometry, point: DrawVector, tolerance: number): boolean {
  const { start: a, end: b } = line, dx = b.x - a.x, dy = b.y - a.y, length = dx * dx + dy * dy;
  const t = length ? Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length)) : 0;
  if (Math.hypot(point.x - a.x - dx * t, point.y - a.y - dy * t) <= tolerance + line.stroke_width / 2) return true;
  return linePaint(line).heads.some(([a, b, c]) => {
    const cross = (u: DrawVector, v: DrawVector) => (point.x - v.x) * (u.y - v.y) - (u.x - v.x) * (point.y - v.y);
    const signs = [cross(a, b), cross(b, c), cross(c, a)];
    return signs.every(n => n >= 0) || signs.every(n => n <= 0);
  });
}
