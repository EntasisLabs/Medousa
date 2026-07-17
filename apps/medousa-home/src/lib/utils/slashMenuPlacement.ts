/**
 * Place the vault slash menu inside an editor shell without clipping edges.
 * Flips above the caret when space below is tight; clamps left and max-height.
 */

export type SlashMenuAnchor = {
  left: number;
  /** Distance from shell top when opening below the caret. */
  top?: number;
  /** Distance from shell bottom when opening above the caret. */
  bottom?: number;
  maxHeight: number;
};

/** Chrome + list max (~max-h-64) — preferred open size. */
const PREFERRED_HEIGHT = 320;
/** Don't bother flipping for a stub smaller than this. */
const MIN_USEFUL_HEIGHT = 140;
const GAP = 6;
const EDGE = 8;
/** Matches w-[min(100%-1rem,22rem)] roughly. */
const MENU_WIDTH = 352;

export type CaretBox = {
  top: number;
  bottom: number;
  left: number;
};

export function placeSlashMenuAnchor(
  caret: CaretBox,
  shell: HTMLElement,
): SlashMenuAnchor {
  const rect = shell.getBoundingClientRect();
  const spaceBelow = rect.bottom - caret.bottom - GAP;
  const spaceAbove = caret.top - rect.top - GAP;

  const openAbove =
    spaceBelow < MIN_USEFUL_HEIGHT && spaceAbove > spaceBelow;

  const available = openAbove ? spaceAbove : spaceBelow;
  const maxHeight = Math.max(
    120,
    Math.min(PREFERRED_HEIGHT, Math.floor(available)),
  );

  let left = caret.left - rect.left;
  const maxLeft = Math.max(EDGE, rect.width - Math.min(MENU_WIDTH, rect.width - EDGE) - EDGE);
  left = Math.max(EDGE, Math.min(left, maxLeft));

  if (openAbove) {
    return {
      left,
      bottom: Math.max(EDGE, rect.bottom - caret.top + GAP),
      maxHeight,
    };
  }

  return {
    left,
    top: Math.max(EDGE, caret.bottom - rect.top + GAP),
    maxHeight,
  };
}
