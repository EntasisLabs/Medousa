/**
 * Place the vault slash menu in the viewport (BodyPortal / position:fixed)
 * without clipping shell edges. Flips above the caret when space below is tight.
 */

export type SlashMenuAnchor = {
  /** Viewport X (CSS `left` for `position: fixed`). */
  left: number;
  /** Viewport Y (CSS `top` for `position: fixed`). */
  top: number;
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
  const viewH =
    typeof window !== "undefined" ? window.innerHeight : rect.bottom + EDGE;
  const viewW =
    typeof window !== "undefined" ? window.innerWidth : rect.right + EDGE;

  // Clamp available space to the intersection of the shell and the viewport.
  const spaceBelow =
    Math.min(rect.bottom, viewH - EDGE) - caret.bottom - GAP;
  const spaceAbove = caret.top - Math.max(rect.top, EDGE) - GAP;

  const openAbove =
    spaceBelow < MIN_USEFUL_HEIGHT && spaceAbove > spaceBelow;

  const available = openAbove ? spaceAbove : spaceBelow;
  const maxHeight = Math.max(
    120,
    Math.min(PREFERRED_HEIGHT, Math.floor(available)),
  );

  let left = caret.left;
  const maxLeft = Math.max(
    EDGE,
    viewW - Math.min(MENU_WIDTH, viewW - EDGE) - EDGE,
  );
  left = Math.max(EDGE, Math.min(left, maxLeft));

  if (openAbove) {
    return {
      left,
      top: Math.max(EDGE, caret.top - GAP - maxHeight),
      maxHeight,
    };
  }

  return {
    left,
    top: Math.min(viewH - EDGE - 40, caret.bottom + GAP),
    maxHeight,
  };
}
