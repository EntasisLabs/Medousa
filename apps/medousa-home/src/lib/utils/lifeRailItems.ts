import type { SurfaceDef } from "$lib/types/environment";
import {
  LME_EXPLORER_MODES,
  type LmeExplorerModeDef,
} from "$lib/utils/lmeExplorerModes";
import { navTier } from "$lib/utils/navSurfaces";

export type LifeRailItem =
  | { kind: "surface"; id: string; surface: SurfaceDef }
  | { kind: "lme-mode"; id: string; mode: LmeExplorerModeDef };

/**
 * Life-orbit rail rows. The Workspace (`library`) surface is expanded into
 * first-class explorer mode destinations at its preset position.
 */
export function buildLifeRailItems(surfaces: SurfaceDef[]): LifeRailItem[] {
  const items: LifeRailItem[] = [];

  for (const surface of surfaces) {
    if (surface.id === "library") {
      // Workspace toggle in Settings controls this whole mode group.
      for (const mode of LME_EXPLORER_MODES) {
        items.push({ kind: "lme-mode", id: `lme:${mode.id}`, mode });
      }
      continue;
    }
    if (navTier(surface) !== "life") continue;
    items.push({ kind: "surface", id: surface.id, surface });
  }

  return items;
}
