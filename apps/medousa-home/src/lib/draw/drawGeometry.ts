import { cloneDrawStroke, type DrawBrush, type DrawPoint, type DrawStroke } from "./drawDocument";
import type { DrawVector } from "./drawCamera";

export type DrawBounds = { x: number; y: number; width: number; height: number };

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function distance(left: DrawVector, right: DrawVector): number {
  return Math.hypot(left.x - right.x, left.y - right.y);
}

function pressureAt(points: DrawPoint[], index: number, brush: DrawBrush): number {
  const direct = points[index]?.pressure;
  if (direct != null) return clamp(direct, 0, 1);
  if (index === 0) return 0.5;
  const previous = points[index - 1];
  const point = points[index];
  const elapsed = Math.max(1, (point.elapsedMs ?? index * 8) - (previous.elapsedMs ?? (index - 1) * 8));
  const velocity = distance(previous, point) / elapsed;
  const velocityPressure = 0.82 - clamp(velocity / Math.max(0.35, brush.size * 0.08), 0, 1) * 0.52;
  return clamp(velocityPressure, 0.25, 0.82);
}

export function drawStrokeRadius(stroke: DrawStroke, index: number): number {
  const pressure = pressureAt(stroke.points, index, stroke.brush);
  const factor = 1 + stroke.brush.thinning * (pressure * 2 - 1);
  return Math.max(stroke.brush.size * 0.12, (stroke.brush.size / 2) * factor);
}

function smoothPoints(points: DrawPoint[], smoothing: number): DrawPoint[] {
  if (points.length < 3 || smoothing <= 0) return points;
  const amount = clamp(smoothing, 0, 1) * 0.35;
  return points.map((point, index) => {
    if (index === 0 || index === points.length - 1) return point;
    const previous = points[index - 1];
    const next = points[index + 1];
    return {
      ...point,
      x: point.x * (1 - amount) + ((previous.x + next.x) / 2) * amount,
      y: point.y * (1 - amount) + ((previous.y + next.y) / 2) * amount,
    };
  });
}

export function drawStrokeOutline(stroke: DrawStroke): DrawVector[] {
  const points = smoothPoints(
    stroke.points,
    stroke.brush.smoothing * 0.7 + stroke.brush.streamline * 0.3,
  );
  if (points.length === 0) return [];
  if (points.length === 1) {
    const radius = drawStrokeRadius(stroke, 0);
    return Array.from({ length: 16 }, (_, index) => {
      const angle = (index / 16) * Math.PI * 2;
      return {
        x: points[0].x + Math.cos(angle) * radius,
        y: points[0].y + Math.sin(angle) * radius,
      };
    });
  }

  const left: DrawVector[] = [];
  const right: DrawVector[] = [];
  for (let index = 0; index < points.length; index += 1) {
    const previous = points[Math.max(0, index - 1)];
    const next = points[Math.min(points.length - 1, index + 1)];
    const dx = next.x - previous.x;
    const dy = next.y - previous.y;
    const magnitude = Math.hypot(dx, dy) || 1;
    const normal = { x: -dy / magnitude, y: dx / magnitude };
    const radius = drawStrokeRadius({ ...stroke, points }, index);
    left.push({ x: points[index].x + normal.x * radius, y: points[index].y + normal.y * radius });
    right.push({ x: points[index].x - normal.x * radius, y: points[index].y - normal.y * radius });
  }
  return [...left, ...right.reverse()];
}

function rounded(value: number): string {
  return String(Math.round(value * 100) / 100);
}

export function drawStrokeOutlinePath(stroke: DrawStroke): string {
  const outline = drawStrokeOutline(stroke);
  if (outline.length === 0) return "";
  let path = `M ${rounded(outline[0].x)} ${rounded(outline[0].y)}`;
  for (let index = 1; index < outline.length; index += 1) {
    const point = outline[index];
    const next = outline[(index + 1) % outline.length];
    path += ` Q ${rounded(point.x)} ${rounded(point.y)} ${rounded((point.x + next.x) / 2)} ${rounded((point.y + next.y) / 2)}`;
  }
  return `${path} Z`;
}

export function simplifyDrawPoints(points: DrawPoint[], tolerance = 0.7): DrawPoint[] {
  if (points.length <= 2) return points.map((point) => ({ ...point }));
  const kept = [{ ...points[0] }];
  for (let index = 1; index < points.length - 1; index += 1) {
    const point = points[index];
    const previous = kept[kept.length - 1];
    const pressureChanged = Math.abs((point.pressure ?? 0.5) - (previous.pressure ?? 0.5)) >= 0.035;
    const tiltChanged =
      Math.abs((point.tiltX ?? 0) - (previous.tiltX ?? 0)) >= 3 ||
      Math.abs((point.tiltY ?? 0) - (previous.tiltY ?? 0)) >= 3;
    if (distance(previous, point) >= tolerance || pressureChanged || tiltChanged) kept.push({ ...point });
  }
  kept.push({ ...points[points.length - 1] });
  return kept;
}

export function drawStrokeBounds(stroke: DrawStroke): DrawBounds {
  if (stroke.points.length === 0) return { x: 0, y: 0, width: 0, height: 0 };
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  stroke.points.forEach((point, index) => {
    const radius = drawStrokeRadius(stroke, index);
    minX = Math.min(minX, point.x - radius);
    minY = Math.min(minY, point.y - radius);
    maxX = Math.max(maxX, point.x + radius);
    maxY = Math.max(maxY, point.y + radius);
  });
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

export function combinedDrawBounds(strokes: DrawStroke[]): DrawBounds | null {
  if (strokes.length === 0) return null;
  const bounds = strokes.map(drawStrokeBounds);
  const x = Math.min(...bounds.map((bound) => bound.x));
  const y = Math.min(...bounds.map((bound) => bound.y));
  const right = Math.max(...bounds.map((bound) => bound.x + bound.width));
  const bottom = Math.max(...bounds.map((bound) => bound.y + bound.height));
  return { x, y, width: right - x, height: bottom - y };
}

function pointToSegmentDistance(point: DrawVector, start: DrawVector, end: DrawVector): number {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  if (dx === 0 && dy === 0) return distance(point, start);
  const t = clamp(((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy), 0, 1);
  return distance(point, { x: start.x + dx * t, y: start.y + dy * t });
}

export function hitTestDrawStroke(stroke: DrawStroke, point: DrawVector, padding = 6): boolean {
  if (stroke.points.length === 1) {
    return distance(stroke.points[0], point) <= drawStrokeRadius(stroke, 0) + padding;
  }
  for (let index = 1; index < stroke.points.length; index += 1) {
    const radius = Math.max(drawStrokeRadius(stroke, index - 1), drawStrokeRadius(stroke, index));
    if (pointToSegmentDistance(point, stroke.points[index - 1], stroke.points[index]) <= radius + padding) {
      return true;
    }
  }
  return false;
}

export function topDrawStrokeAt(strokes: DrawStroke[], point: DrawVector, padding = 6): DrawStroke | null {
  for (let index = strokes.length - 1; index >= 0; index -= 1) {
    if (hitTestDrawStroke(strokes[index], point, padding)) return strokes[index];
  }
  return null;
}

export function pointInDrawPolygon(point: DrawVector, polygon: DrawVector[]): boolean {
  if (polygon.length < 3) return false;
  let inside = false;
  for (let index = 0, previous = polygon.length - 1; index < polygon.length; previous = index++) {
    const a = polygon[index];
    const b = polygon[previous];
    const intersects =
      a.y > point.y !== b.y > point.y &&
      point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y || Number.EPSILON) + a.x;
    if (intersects) inside = !inside;
  }
  return inside;
}

export function selectDrawStrokesInLasso(strokes: DrawStroke[], polygon: DrawVector[]): string[] {
  return strokes
    .filter((stroke) => {
      if (stroke.points.some((point) => pointInDrawPolygon(point, polygon))) return true;
      for (let strokeIndex = 1; strokeIndex < stroke.points.length; strokeIndex += 1) {
        for (let polygonIndex = 0; polygonIndex < polygon.length; polygonIndex += 1) {
          const nextPolygonIndex = (polygonIndex + 1) % polygon.length;
          if (segmentsIntersect(
            stroke.points[strokeIndex - 1],
            stroke.points[strokeIndex],
            polygon[polygonIndex],
            polygon[nextPolygonIndex],
          )) return true;
        }
      }
      return false;
    })
    .map((stroke) => stroke.id);
}

function segmentsIntersect(a: DrawVector, b: DrawVector, c: DrawVector, d: DrawVector): boolean {
  if (
    Math.max(a.x, b.x) < Math.min(c.x, d.x) ||
    Math.max(c.x, d.x) < Math.min(a.x, b.x) ||
    Math.max(a.y, b.y) < Math.min(c.y, d.y) ||
    Math.max(c.y, d.y) < Math.min(a.y, b.y)
  ) return false;
  const cross = (first: DrawVector, second: DrawVector, third: DrawVector) =>
    (second.x - first.x) * (third.y - first.y) - (second.y - first.y) * (third.x - first.x);
  const abC = cross(a, b, c);
  const abD = cross(a, b, d);
  const cdA = cross(c, d, a);
  const cdB = cross(c, d, b);
  return (
    ((abC <= 0 && abD >= 0) || (abC >= 0 && abD <= 0)) &&
    ((cdA <= 0 && cdB >= 0) || (cdA >= 0 && cdB <= 0))
  );
}

function distanceToPath(point: DrawVector, path: DrawVector[]): number {
  if (path.length === 0) return Number.POSITIVE_INFINITY;
  if (path.length === 1) return distance(point, path[0]);
  let best = Number.POSITIVE_INFINITY;
  for (let index = 1; index < path.length; index += 1) {
    best = Math.min(best, pointToSegmentDistance(point, path[index - 1], path[index]));
  }
  return best;
}

export function eraseDrawStrokeByPath(
  stroke: DrawStroke,
  eraserPath: DrawVector[],
  radius: number,
): DrawStroke[] {
  if (stroke.points.length === 0 || eraserPath.length === 0) return [cloneDrawStroke(stroke)];
  const points = densifyDrawPoints(stroke.points, Math.max(1, radius * 0.5));
  const groups: DrawPoint[][] = [];
  let active: DrawPoint[] = [];
  points.forEach((point) => {
    const erased =
      distanceToPath(point, eraserPath) <=
      radius + drawStrokeRadius({ ...stroke, points: [point] }, 0);
    if (erased) {
      if (active.length > 0) groups.push(active);
      active = [];
    } else {
      active.push({ ...point });
    }
  });
  if (active.length > 0) groups.push(active);
  if (groups.length === 1 && groups[0].length === points.length) return [cloneDrawStroke(stroke)];
  return groups
    .filter((points) => points.length > 0)
    .map((points, index) => ({
      ...cloneDrawStroke(stroke),
      id: `part:${index}:${Math.round(points[0].x * 10)}:${Math.round(points[0].y * 10)}:${stroke.id}`.slice(0, 128),
      points: simplifyDrawPoints(points, 0.5),
    }));
}

function interpolateOptional(left: number | undefined, right: number | undefined, t: number): number | undefined {
  if (left == null || right == null) return undefined;
  return left + (right - left) * t;
}

function densifyDrawPoints(points: DrawPoint[], spacing: number): DrawPoint[] {
  if (points.length < 2) return points.map((point) => ({ ...point }));
  const dense = [{ ...points[0] }];
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const point = points[index];
    const steps = Math.min(64, Math.max(1, Math.ceil(distance(previous, point) / spacing)));
    for (let step = 1; step <= steps; step += 1) {
      const t = step / steps;
      const pressure = interpolateOptional(previous.pressure, point.pressure, t);
      const elapsedMs = interpolateOptional(previous.elapsedMs, point.elapsedMs, t);
      const tiltX = interpolateOptional(previous.tiltX, point.tiltX, t);
      const tiltY = interpolateOptional(previous.tiltY, point.tiltY, t);
      dense.push({
        x: previous.x + (point.x - previous.x) * t,
        y: previous.y + (point.y - previous.y) * t,
        ...(pressure != null ? { pressure } : {}),
        ...(elapsedMs != null ? { elapsedMs } : {}),
        ...(tiltX != null ? { tiltX } : {}),
        ...(tiltY != null ? { tiltY } : {}),
      });
    }
  }
  return dense;
}

export function moveDrawStroke(stroke: DrawStroke, delta: DrawVector): DrawStroke {
  return {
    ...cloneDrawStroke(stroke),
    points: stroke.points.map((point) => ({ ...point, x: point.x + delta.x, y: point.y + delta.y })),
  };
}
