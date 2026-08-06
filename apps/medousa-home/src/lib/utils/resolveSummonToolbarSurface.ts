import type { LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
import {
  familyForLmeExplorerMode,
  familyForLmeTabKind,
} from "$lib/utils/lmeExplorerModes";
import { surfaceHasShellSidebarView } from "$lib/utils/navSurfaces";

function surfaceForLibraryMode(mode: LmeExplorerMode): string {
  if (mode === "files") return "files";
  if (mode === "artifacts") return "artifacts";
  return "notes";
}

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
    desktopSurface !== "notes" &&
    desktopSurface !== "files" &&
    desktopSurface !== "artifacts" &&
    desktopSurface !== "automations"
  ) {
    return desktopSurface;
  }

  // Workspace / LME host — pick Notes/Files/Artifacts vs Automations from the open tab.
  if (
    desktopSurface === "library" ||
    desktopSurface === "notes" ||
    desktopSurface === "files" ||
    desktopSurface === "artifacts" ||
    desktopSurface === "code" ||
    desktopSurface === "automations" ||
    desktopSurface === "workshop"
  ) {
    const fromTab = activeLmeKind ? familyForLmeTabKind(activeLmeKind) : null;
    const family = fromTab ?? familyForLmeExplorerMode(explorerMode);
    if (family === "library") {
      if (activeLmeKind === "file") return "files";
      if (activeLmeKind === "deck") return "artifacts";
      if (activeLmeKind === "note") return "notes";
      return surfaceForLibraryMode(explorerMode);
    }
    return family;
  }

  return null;
}
