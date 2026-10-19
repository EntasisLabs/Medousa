import type { SurfaceDef } from "$lib/types/environment";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";

const LIFE_IDS = new Set([
  "chat",
  "code",
  "reviews",
  "notes",
  "files",
  "artifacts",
  "calendar",
  "web",
  "map",
  "peers",
]);
const WORKSHOP_IDS = new Set(["workshop"]);
/** Runtime + Messaging live under Settings — never the life rail. */
const HIDDEN_IDS = new Set([SAFETY_SURFACE_RUNTIME, "messaging", "work"]);

/** Surfaces whose list chrome lives in the master left rail (view mode). */
export const SHELL_SIDEBAR_VIEW_SURFACES = new Set([
  "chat",
  "library",
  "notes",
  "files",
  "artifacts",
  "code",
  "reviews",
  "automations",
  "workshop",
  "peers",
  "messaging",
  "map",
  "calendar",
  "web",
  "profiles",
  SAFETY_SURFACE_SETTINGS,
]);

/** Surfaces hosted by the shared LME explorer in the side rail. */
export function shellSidebarUsesLmeExplorer(surfaceId: string): boolean {
  return ["library", "notes", "files", "artifacts", "automations", "code"].includes(surfaceId);
}

export function surfaceHasShellSidebarView(surfaceId: string): boolean {
  if (surfaceId === "automations") return true;
  return SHELL_SIDEBAR_VIEW_SURFACES.has(surfaceId);
}

export function navTier(surface: SurfaceDef): "life" | "workshop" | "utility" | "hidden" {
  // Workspace host stays real for LME/shell tabs; rail shows Notes/Files/Artifacts instead.
  if (
    surface.id === "library" ||
    surface.id === "automations" ||
    surface.id === "workshop"
  ) {
    return "hidden";
  }
  if (surface.id === "home" || surface.id === SAFETY_SURFACE_SETTINGS) return "hidden";
  if (HIDDEN_IDS.has(surface.id)) return "hidden";
  if (surface.kind === "custom") return "life";
  if (WORKSHOP_IDS.has(surface.id)) return "workshop";
  if (LIFE_IDS.has(surface.id)) return "life";
  return "life";
}

export function navTitle(surface: SurfaceDef): string {
  if (surface.id === "workshop") return "Agents";
  if (surface.id === "automations") return "Automations";
  if (surface.id === "map") return "Context";
  if (surface.id === "peers") return "Peers";
  if (surface.id === "profiles") return "You";
  return surface.label;
}

export function navLabel(surface: SurfaceDef): string {
  if (surface.id === "workshop") return "Agents";
  if (surface.id === "automations") return "Automations";
  if (surface.id === "map") return "Context";
  if (surface.id === "profiles") return "You";
  return surface.label;
}

export function shellSidebarViewTitle(surfaceId: string): string {
  switch (surfaceId) {
    case "chat":
      return "Sessions";
    case "library":
    case "notes":
      return "Notes";
    case "files":
      return "Files";
    case "artifacts":
      return "Artifacts";
    case "code":
      return "Projects";
    case "reviews":
      return "Reviews";
    case "workshop":
      return "Agents";
    case "automations":
      return "Automations";
    case "peers":
      return "Peers";
    case "messaging":
      return "Channels";
    case "map":
      return "Context";
    case "calendar":
      return "Calendar";
    case "web":
      return "Web";
    case "profiles":
      return "You";
    case SAFETY_SURFACE_SETTINGS:
      return "Settings";
    default:
      return "Sidebar";
  }
}
