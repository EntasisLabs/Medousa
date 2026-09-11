import {
  cloneDrawDocument,
  cloneDrawStroke,
  type DrawDocument,
  type DrawStroke,
} from "./drawDocument";

export type DrawStrokeSnapshot = { index: number; stroke: DrawStroke };

export type DrawPatch = {
  label: string;
  before: DrawStrokeSnapshot[];
  after: DrawStrokeSnapshot[];
};

function sameStroke(left: DrawStroke | undefined, right: DrawStroke | undefined): boolean {
  return left != null && right != null && JSON.stringify(left) === JSON.stringify(right);
}

export function createDrawPatch(
  before: DrawDocument,
  after: DrawDocument,
  label: string,
): DrawPatch | null {
  const beforeById = new Map(before.strokes.map((stroke, index) => [stroke.id, { stroke, index }]));
  const afterById = new Map(after.strokes.map((stroke, index) => [stroke.id, { stroke, index }]));
  const ids = new Set([...beforeById.keys(), ...afterById.keys()]);
  const changed = [...ids].filter((id) => !sameStroke(beforeById.get(id)?.stroke, afterById.get(id)?.stroke));
  if (changed.length === 0) return null;
  return {
    label,
    before: changed.flatMap((id) => {
      const value = beforeById.get(id);
      return value ? [{ index: value.index, stroke: cloneDrawStroke(value.stroke) }] : [];
    }),
    after: changed.flatMap((id) => {
      const value = afterById.get(id);
      return value ? [{ index: value.index, stroke: cloneDrawStroke(value.stroke) }] : [];
    }),
  };
}

export function applyDrawPatch(
  document: DrawDocument,
  patch: DrawPatch,
  direction: "undo" | "redo",
): DrawDocument {
  const removeIds = new Set([...patch.before, ...patch.after].map((entry) => entry.stroke.id));
  const strokes = document.strokes
    .filter((stroke) => !removeIds.has(stroke.id))
    .map(cloneDrawStroke);
  const insertions = direction === "undo" ? patch.before : patch.after;
  for (const entry of [...insertions].sort((left, right) => left.index - right.index)) {
    strokes.splice(Math.max(0, Math.min(strokes.length, entry.index)), 0, cloneDrawStroke(entry.stroke));
  }
  return { ...cloneDrawDocument(document), strokes };
}
