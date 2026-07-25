import type { SurfaceDef } from "$lib/types/environment";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";
import type { LifeRailItem } from "$lib/utils/lifeRailItems";

/**
 * Jobs rail — open doors, not a table of contents.
 * Library (vault) and Automations are separate destinations; modes live inside.
 * Context and You are sibling dock doors at the bottom (not nested).
 */
export type LifeRailLayout = {
  primary: LifeRailItem[];
  /** First index in `primary` that belongs to the focus strip (calendar…); -1 if none. */
  focusStartIndex: number;
  /** First custom surface index in `primary`; -1 if none. */
  customStartIndex: number;
  /** Show Library door after the focus strip (before custom). */
  showLibrary: boolean;
  /** Show Automations door next to Library. */
  showAutomations: boolean;
  you: LifeRailItem;
  /** Dock sibling next to You (own door, not nested). */
  context: LifeRailItem | null;
};

/** @deprecated Diagnostics / legacy mapping only. */
export type RailSectionId =
  | "primary"
  | "library"
  | "custom"
  | "memory"
  | "channels"
  | "focus"
  | "vault"
  | "automations";

const FOCUS_IDS = new Set(["calendar", "work", "web"]);

/** Dock / chrome / folded doors — never appear in the primary strip. */
const RAIL_PRIMARY_SKIP_IDS = new Set([
  "workshop",
  "home",
  "context",
  "profiles",
  "messaging",
  SAFETY_SURFACE_SETTINGS,
  SAFETY_SURFACE_RUNTIME,
]);

/** Synthetic Library row — vault modes switch inside the surface, not on the rail. */
export function libraryRailSurface(): SurfaceDef {
  return {
    id: "library",
    label: "Library",
    icon: "book-open",
    kind: "builtin",
    builtinId: "library",
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

/** Synthetic Automations row — run modes switch inside the surface, not on the rail. */
export function automationsRailSurface(): SurfaceDef {
  return {
    id: "automations",
    label: "Automations",
    icon: "zap",
    kind: "builtin",
    builtinId: "automations",
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

export function profilesRailSurface(): SurfaceDef {
  return {
    id: "profiles",
    label: "You",
    icon: "user",
    kind: "builtin",
    builtinId: "profiles",
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

/**
 * Compact life-rail layout. Runtime / Settings stay off this list.
 * Primary order follows the given `surfaces` sequence (active layout preset).
 */
export function buildLifeRailLayout(surfaces: SurfaceDef[]): LifeRailLayout {
  const byId = new Map(surfaces.map((surface) => [surface.id, surface]));
  const primary: LifeRailItem[] = [];
  let sawLibrary = false;
  let sawAutomations = false;

  for (const surface of surfaces) {
    if (RAIL_PRIMARY_SKIP_IDS.has(surface.id)) continue;

    if (surface.id === "library") {
      sawLibrary = true;
      primary.push({ kind: "surface", id: "library", surface: libraryRailSurface() });
      continue;
    }
    if (surface.id === "automations") {
      sawAutomations = true;
      primary.push({
        kind: "surface",
        id: "automations",
        surface: automationsRailSurface(),
      });
      continue;
    }

    primary.push({ kind: "surface", id: surface.id, surface });
  }

  // Twin door: Library implies Automations even when older presets dropped it.
  if (sawLibrary && !sawAutomations) {
    const libraryAt = primary.findIndex((item) => item.id === "library");
    if (libraryAt >= 0) {
      primary.splice(libraryAt + 1, 0, {
        kind: "surface",
        id: "automations",
        surface: automationsRailSurface(),
      });
    }
  }

  const focusStartIndex = primary.findIndex((item) => FOCUS_IDS.has(item.id));
  const customStartIndex = primary.findIndex(
    (item) => item.kind === "surface" && item.surface.kind === "custom",
  );

  const contextSurface = byId.get("context") ?? null;
  const profilesExisting = byId.get("profiles");
  const you: LifeRailItem = {
    kind: "surface",
    id: "profiles",
    surface: profilesExisting
      ? {
          ...profilesExisting,
          label: profilesExisting.label === "Profiles" ? "You" : profilesExisting.label,
        }
      : profilesRailSurface(),
  };

  return {
    primary,
    focusStartIndex,
    customStartIndex,
    showLibrary: sawLibrary,
    showAutomations: sawLibrary || sawAutomations,
    you,
    context: contextSurface
      ? { kind: "surface", id: "context", surface: contextSurface }
      : null,
  };
}

/** Legacy section list — membership diagnostics only. */
export function buildLifeRailSections(surfaces: SurfaceDef[]): {
  id: RailSectionId;
  label: string;
  items: LifeRailItem[];
}[] {
  const layout = buildLifeRailLayout(surfaces);
  const talk = layout.primary.filter(
    (item) =>
      !FOCUS_IDS.has(item.id) &&
      item.id !== "library" &&
      item.id !== "automations" &&
      !(item.kind === "surface" && item.surface.kind === "custom"),
  );
  const focus = layout.primary.filter((item) => FOCUS_IDS.has(item.id));
  const custom = layout.primary.filter(
    (item) => item.kind === "surface" && item.surface.kind === "custom",
  );
  const sections: { id: RailSectionId; label: string; items: LifeRailItem[] }[] = [];
  if (talk.length) sections.push({ id: "channels", label: "Channels", items: talk });
  if (focus.length) sections.push({ id: "focus", label: "Focus", items: focus });
  if (layout.showLibrary || layout.showAutomations) {
    const items = layout.primary.filter(
      (item) => item.id === "library" || item.id === "automations",
    );
    sections.push({
      id: layout.showLibrary ? "library" : "automations",
      label: layout.showLibrary ? "Library" : "Automations",
      items,
    });
  }
  const memory: LifeRailItem[] = [];
  if (layout.context) memory.push(layout.context);
  memory.push(layout.you);
  sections.push({ id: "memory", label: "Memory", items: memory });
  if (custom.length) sections.push({ id: "custom", label: "Custom", items: custom });
  return sections;
}

export function railSectionForItemId(itemId: string): RailSectionId | null {
  if (itemId === "library" || itemId.startsWith("lme:")) return "library";
  if (itemId === "automations") return "automations";
  if (itemId === "chat" || itemId === "peers" || itemId === "messaging") {
    return "channels";
  }
  if (FOCUS_IDS.has(itemId)) return "focus";
  if (itemId === "context" || itemId === "profiles") return "memory";
  return null;
}
