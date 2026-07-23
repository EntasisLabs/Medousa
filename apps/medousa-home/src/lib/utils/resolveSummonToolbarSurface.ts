import type { LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
import { familyForLmeExplorerMode } from "$lib/utils/lmeExplorerModes";
import { surfaceHasShellSidebarView } from "$lib/utils/navSurfaces";

/**
 * Which rail-popover surface to summon for the current desktop + LME explorer mode.
 * Returns null when the active surface has no list toolbar chrome.
 */
export function resolveSummonToolbarSurface(
  desktopSurface: string,
  explorerMode: LmeExplorerMode,
): string | null {
  // Non-LME list surfaces win over leftover explorer mode (e.g. chat after notes).
  if (
    surfaceHasShellSidebarView(desktopSurface) &&
    desktopSurface !== "library" &&
    desktopSurface !== "automations"
  ) {
    return desktopSurface;
  }

  // Workspace / LME host — pick Library vs Automations from explorer mode.
  if (
    desktopSurface === "library" ||
    desktopSurface === "automations" ||
    desktopSurface === "workshop"
  ) {
    return familyForLmeExplorerMode(explorerMode);
  }

  return null;
}
