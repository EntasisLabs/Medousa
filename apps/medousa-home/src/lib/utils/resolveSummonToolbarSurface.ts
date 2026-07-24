import type { LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
import {
  familyForLmeExplorerMode,
  familyForLmeTabKind,
} from "$lib/utils/lmeExplorerModes";
import { surfaceHasShellSidebarView } from "$lib/utils/navSurfaces";

/**
 * Which rail-popover surface to summon for the current desktop + LME context.
 * Returns null when the active surface has no list toolbar chrome.
 *
 * `activeLmeKind` wins over `explorerMode` for Library/Automations — tab activation
 * intentionally does not sync explorer mode, so mode alone is often stale.
 */
export function resolveSummonToolbarSurface(
  desktopSurface: string,
  explorerMode: LmeExplorerMode,
  activeLmeKind?: string | null,
): string | null {
  // Non-LME list surfaces win over leftover explorer mode (e.g. chat after notes).
  if (
    surfaceHasShellSidebarView(desktopSurface) &&
    desktopSurface !== "library" &&
    desktopSurface !== "automations"
  ) {
    return desktopSurface;
  }

  // Workspace / LME host — pick Library vs Automations from the open tab when possible.
  if (
    desktopSurface === "library" ||
    desktopSurface === "automations" ||
    desktopSurface === "workshop"
  ) {
    const fromTab = activeLmeKind ? familyForLmeTabKind(activeLmeKind) : null;
    if (fromTab) return fromTab;
    return familyForLmeExplorerMode(explorerMode);
  }

  return null;
}
