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
  /** Whether the menu opens above the caret/input. */
  placement?: "above" | "below";
};

/** Chrome + list max (~max-h-64) — preferred open size. */
const PREFERRED_HEIGHT = 320;
/** Composer menus prefer a slightly taller panel when space allows. */
const COMPOSER_PREFERRED_HEIGHT = 360;
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

function clampMenuLeft(left: number, viewW: number): number {
  const maxLeft = Math.max(
    EDGE,
    viewW - Math.min(MENU_WIDTH, viewW - EDGE) - EDGE,
  );
  return Math.max(EDGE, Math.min(left, maxLeft));
}

function heightForSpace(available: number, preferred: number): number {
  // Never claim more height than the open side actually has (avoids viewport clip).
  if (available < 72) return Math.max(0, Math.floor(available));
  return Math.max(72, Math.min(preferred, Math.floor(available)));
}

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
  const maxHeight = heightForSpace(available, PREFERRED_HEIGHT);
  const left = clampMenuLeft(caret.left, viewW);

  if (openAbove) {
    return {
      left,
      top: Math.max(EDGE, caret.top - GAP - maxHeight),
      maxHeight,
      placement: "above",
    };
  }

  return {
    left,
    top: Math.min(viewH - EDGE - Math.min(40, maxHeight), caret.bottom + GAP),
    maxHeight,
    placement: "below",
  };
}

/**
 * Composer / dock slash menus: ignore the host form shell and use the viewport.
 * Prefer opening above when the input sits in the lower half (dock / chat footer).
 */
export function placeComposerSlashMenuAnchor(caret: CaretBox): SlashMenuAnchor {
  const viewH = typeof window !== "undefined" ? window.innerHeight : 800;
  const viewW = typeof window !== "undefined" ? window.innerWidth : 1200;

  const spaceBelow = viewH - EDGE - caret.bottom - GAP;
  const spaceAbove = caret.top - EDGE - GAP;
  const inLowerHalf = caret.bottom > viewH * 0.45;

  const openAbove =
    (inLowerHalf && spaceAbove >= 120) ||
    (spaceBelow < MIN_USEFUL_HEIGHT && spaceAbove > spaceBelow) ||
    (spaceAbove > spaceBelow && spaceBelow < COMPOSER_PREFERRED_HEIGHT);

  const available = openAbove ? spaceAbove : spaceBelow;
  const maxHeight = heightForSpace(available, COMPOSER_PREFERRED_HEIGHT);
  const left = clampMenuLeft(caret.left, viewW);

  if (openAbove) {
    const top = Math.max(EDGE, caret.top - GAP - maxHeight);
    // Re-clamp height so top + height never past the caret.
    const fitHeight = Math.min(maxHeight, Math.max(0, caret.top - GAP - top));
    return {
      left,
      top,
      maxHeight: fitHeight,
      placement: "above",
    };
  }

  const top = caret.bottom + GAP;
  const fitHeight = Math.min(maxHeight, Math.max(0, viewH - EDGE - top));
  return {
    left,
    top,
    maxHeight: fitHeight,
    placement: "below",
  };
}
