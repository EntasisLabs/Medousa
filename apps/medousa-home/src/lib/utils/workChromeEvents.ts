/** Shell entry for background asks; never changes the current destination. */
import { layout } from "$lib/runtime/layout.svelte";
import { taskOverlays } from "$lib/runtime/taskOverlayState.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";

export function openNewAsk() {
  commandSpotlight.closeSpotlight();
  taskOverlays.askHistorySessionId = null;
  if (layout.isMobile) layout.setAskSheetOpen(true);
  else taskOverlays.newAskOpen = true;
}
