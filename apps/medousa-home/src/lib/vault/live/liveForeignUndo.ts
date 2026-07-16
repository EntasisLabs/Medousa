/**
 * Cross-file undo for embed write-through (Live edits another note's body).
 * TipTap's history only covers the host note — this stack restores the source.
 */

export type ForeignUndoEntry = {
  path: string;
  content: string;
  at: number;
};

const MAX = 40;
const stack: ForeignUndoEntry[] = [];
let armedUntil = 0;

export function pushForeignUndo(path: string, previousContent: string): void {
  const clean = path.trim();
  if (!clean) return;
  stack.push({ path: clean, content: previousContent, at: Date.now() });
  if (stack.length > MAX) stack.shift();
  armedUntil = Date.now() + 12_000;
}

export function foreignUndoArmed(): boolean {
  return stack.length > 0 && Date.now() <= armedUntil;
}

export function takeForeignUndo(): ForeignUndoEntry | null {
  if (!foreignUndoArmed()) return null;
  const entry = stack.pop() ?? null;
  if (stack.length === 0) armedUntil = 0;
  return entry;
}

export function clearForeignUndo(): void {
  stack.length = 0;
  armedUntil = 0;
}
