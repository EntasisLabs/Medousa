/**
 * LME explorer docks live in their side-rail footers by default.
 * Rail popovers temporarily host them in the toolbar via push/pop.
 */

/** Compact chrome class while the dock sits in a popover toolbar (legacy name). */
const HOSTED_CLASS = "lme-side-rail-dock--status";

/** @deprecated Always false — docks no longer live in the status bar. */
export const LME_DOCK_IN_STATUS_BAR = false;

/** Where the dock returns when no overlay host is active. */
let homeParent: HTMLElement | null = null;
/** Temporary hosts (rail popover dock slots), top of stack is active. */
const overlayStack: HTMLElement[] = [];
let activeDock: HTMLElement | null = null;

function overlayHost(): HTMLElement | null {
  return overlayStack.length > 0
    ? (overlayStack[overlayStack.length - 1] ?? null)
    : null;
}

function applyActiveHost() {
  if (!activeDock) return;
  const host = overlayHost();
  if (host) {
    activeDock.classList.add(HOSTED_CLASS);
    if (activeDock.parentElement !== host) {
      host.appendChild(activeDock);
    }
    return;
  }

  activeDock.classList.remove(HOSTED_CLASS);
  if (homeParent && activeDock.parentElement !== homeParent) {
    homeParent.appendChild(activeDock);
  }
}

/**
 * @deprecated Status-bar hosting removed. Kept as a no-op so callers compile.
 */
export function setLmeDockHost(_el: HTMLElement | null) {
  // Intentionally ignored — docks stay in rail footers unless a popover overlays.
}

export function getLmeDockHost(): HTMLElement | null {
  return overlayHost() ?? homeParent;
}

/** Temporarily host docks in another slot (e.g. rail popover toolbar). */
export function pushLmeDockHost(el: HTMLElement) {
  if (overlayHost() === el) {
    applyActiveHost();
    return;
  }
  overlayStack.push(el);
  applyActiveHost();
}

/** Restore the dock to its side-rail footer (or prior overlay). */
export function popLmeDockHost() {
  overlayStack.pop();
  applyActiveHost();
}

/**
 * Svelte action — registers an LME dock footer.
 * Moves into a popover overlay when one is pushed; otherwise stays put.
 */
export function portLmeDock(node: HTMLElement) {
  activeDock = node;
  homeParent = node.parentElement;
  applyActiveHost();

  return {
    destroy() {
      if (activeDock === node) {
        activeDock = null;
      }
      node.classList.remove(HOSTED_CLASS);
      // If still sitting in an overlay host, put it back so Svelte can unmount.
      if (homeParent && node.parentElement !== homeParent) {
        homeParent.appendChild(node);
      }
      if (homeParent === node.parentElement) {
        homeParent = null;
      }
    },
  };
}
