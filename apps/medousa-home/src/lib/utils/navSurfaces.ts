import type { SurfaceDef } from "$lib/types/environment";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";

const LIFE_IDS = new Set(["chat", "work", "library", "calendar", "web", "context", "peers"]);
const WORKSHOP_IDS = new Set(["workshop"]);
const UTILITY_IDS = new Set(["messaging", SAFETY_SURFACE_RUNTIME]);

/** Surfaces whose list chrome lives in the master left rail (view mode). */
export const SHELL_SIDEBAR_VIEW_SURFACES = new Set([
  "chat",
  "library",
  "automations",
  "peers",
  "messaging",
  "context",
  SAFETY_SURFACE_SETTINGS,
]);

export function surfaceHasShellSidebarView(surfaceId: string): boolean {
  if (surfaceId === "automations") return true;
  return SHELL_SIDEBAR_VIEW_SURFACES.has(surfaceId);
}

export function navTier(surface: SurfaceDef): "life" | "workshop" | "utility" | "hidden" {
  if (surface.id === "automations" || surface.id === "workshop") return "hidden";
  if (surface.id === "home" || surface.id === SAFETY_SURFACE_SETTINGS) return "hidden";
  if (surface.kind === "custom") return "life";
  if (WORKSHOP_IDS.has(surface.id)) return "workshop";
  if (UTILITY_IDS.has(surface.id)) return "utility";
  if (LIFE_IDS.has(surface.id)) return "life";
  return "life";
}

export function navTitle(surface: SurfaceDef): string {
  if (surface.id === "library") return "Workspace";
  if (surface.id === "context") return "Threads & memory";
  if (surface.id === "peers") return "Peers";
  return surface.label;
}

export function navLabel(surface: SurfaceDef): string {
  if (surface.id === "library") return "Workspace";
  if (surface.id === "context") return "Context";
  return surface.label;
}

export function shellSidebarViewTitle(surfaceId: string): string {
  switch (surfaceId) {
    case "chat":
      return "Sessions";
    case "library":
    case "automations":
      return "Workspace";
    case "peers":
      return "Peers";
    case "messaging":
      return "Channels";
    case "context":
      return "Context";
    case SAFETY_SURFACE_SETTINGS:
      return "Settings";
    default:
      return "Sidebar";
  }
}
