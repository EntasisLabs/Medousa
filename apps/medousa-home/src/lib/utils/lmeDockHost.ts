/**
 * Experiment: host LME side-rail docks in the app StatusBar.
 * Flip {@link LME_DOCK_IN_STATUS_BAR} to restore docks to the rail footers.
 */

/** When false, `portLmeDock` is a no-op and docks stay in the side rail. */
export const LME_DOCK_IN_STATUS_BAR = true;

const STATUS_CLASS = "lme-side-rail-dock--status";

let hostEl: HTMLElement | null = null;
let activeDock: HTMLElement | null = null;

export function setLmeDockHost(el: HTMLElement | null) {
  hostEl = el;
  if (activeDock && hostEl && activeDock.parentElement !== hostEl) {
    hostEl.appendChild(activeDock);
  }
}

/** Svelte action — moves an LME dock footer into the status-bar slot. */
export function portLmeDock(node: HTMLElement) {
  if (!LME_DOCK_IN_STATUS_BAR) {
    return {};
  }

  activeDock = node;
  node.classList.add(STATUS_CLASS);
  if (hostEl) {
    hostEl.appendChild(node);
  }

  return {
    destroy() {
      if (activeDock === node) {
        activeDock = null;
      }
      node.classList.remove(STATUS_CLASS);
      // Detach if still in the status host; Svelte also removes on unmount.
      if (node.parentElement === hostEl) {
        node.remove();
      }
    },
  };
}
