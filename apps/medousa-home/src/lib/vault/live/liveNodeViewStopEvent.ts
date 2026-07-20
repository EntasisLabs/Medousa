/**
 * TipTap/ProseMirror steals mousedown on atom node views for NodeSelection,
 * which suppresses click on buttons/tabs inside Liquid surfaces.
 *
 * Return true so the browser (and Svelte) own the event — except `click`,
 * which VaultLiveEditor still routes for Configure / wikilinks.
 * Direct hits on the node-view root still allow atom selection — unless the
 * user is drag-selecting text inside a fence / code body.
 */

const FENCE_SELECTABLE =
  ".vault-live-plain-fence__body, .vault-live-code__stage, .liquid-code-pre, .liquid-code-pre code, .vault-live-fence-raw, .vault-live-fence-raw-shell";

function selectionInsideFence(): boolean {
  if (typeof window === "undefined") return false;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return false;
  const node = sel.anchorNode;
  const el = node instanceof Element ? node : node?.parentElement;
  return Boolean(el?.closest(FENCE_SELECTABLE));
}

export function liveNodeViewStopEvent(event: Event): boolean {
  if (event.type === "click") return false;
  const target = event.target;
  if (!(target instanceof Element)) return true;

  // Native text selection inside rendered fence / code.
  if (target.closest(FENCE_SELECTABLE)) return true;

  // mouseup often lands on the atom host after a drag — don't wipe the selection.
  if (
    (event.type === "mouseup" ||
      event.type === "mousemove" ||
      event.type === "pointerup" ||
      event.type === "pointermove") &&
    selectionInsideFence()
  ) {
    return true;
  }

  if (
    target.hasAttribute("data-fence-block") ||
    target.hasAttribute("data-embed-block") ||
    target.classList.contains("vault-live-organism-host")
  ) {
    return false;
  }
  return true;
}
