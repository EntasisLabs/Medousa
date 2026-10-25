import { publishCreativeChange } from "$lib/create/creativeWindowSync";
import { activeWorkshopId } from "./workshopLocality";
import { isDesktopWindow, openDesktopWindow } from "$lib/desktop/windows";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
/** User-clicked tool result; never opens the main shell for a detached launcher. */
export async function openCreativeResult(id: string) {
  const workshopId = activeWorkshopId();
  publishCreativeChange(workshopId, id);
  await import("$lib/create/creativeWorkspace.svelte").then(({ refreshCreativeProject }) => refreshCreativeProject(workshopId, id)).catch(() => {});
  if (isDesktopWindow()) await openDesktopWindow({ kind: "create", workshopId, resourceId: id });
  else shellTabs.openCreative(id, workshopId);
  commandSpotlight.closeSpotlight();
}
