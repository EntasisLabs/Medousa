/**
 * TipTap/ProseMirror steals mousedown on atom node views for NodeSelection,
 * which suppresses click on buttons/tabs inside Liquid surfaces.
 *
 * Return true so the browser (and Svelte) own the event — except `click`,
 * which VaultLiveEditor still routes for Configure / wikilinks.
 * Direct hits on the node-view root still allow atom selection.
 */
export function liveNodeViewStopEvent(event: Event): boolean {
  if (event.type === "click") return false;
  const target = event.target;
  if (!(target instanceof Element)) return true;
  if (
    target.hasAttribute("data-fence-block") ||
    target.hasAttribute("data-embed-block") ||
    target.classList.contains("vault-live-organism-host")
  ) {
    return false;
  }
  return true;
}
