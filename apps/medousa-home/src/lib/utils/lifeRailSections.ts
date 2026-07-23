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

const PRIMARY_ORDER = [
  "chat",
  "peers",
  "messaging",
  "calendar",
  "work",
  "web",
] as const;

const FOCUS_IDS = new Set(["calendar", "work", "web"]);

const RAIL_SKIP_IDS = new Set([
  "library",
  "automations",
  "workshop",
  "home",
  "context",
  "profiles",
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
 */
export function buildLifeRailLayout(surfaces: SurfaceDef[]): LifeRailLayout {
  const byId = new Map(surfaces.map((surface) => [surface.id, surface]));
  const primary: LifeRailItem[] = [];

  for (const id of PRIMARY_ORDER) {
    const surface = byId.get(id);
    if (!surface || RAIL_SKIP_IDS.has(surface.id)) continue;
    primary.push({ kind: "surface", id: surface.id, surface });
  }

  const focusStartIndex = primary.findIndex((item) => FOCUS_IDS.has(item.id));
  const showLibrary = surfaces.some((surface) => surface.id === "library");
  // Twin door to Library — don't require Automations in the preset list.
  // Existing environments often dropped it when modes lived under Library.
  const showAutomations =
    showLibrary || surfaces.some((surface) => surface.id === "automations");

  const custom: LifeRailItem[] = [];
  for (const surface of surfaces) {
    if (RAIL_SKIP_IDS.has(surface.id)) continue;
    if (surface.kind !== "custom") continue;
    custom.push({ kind: "surface", id: surface.id, surface });
  }

  const customStartIndex = custom.length > 0 ? primary.length : -1;
  primary.push(...custom);

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
    showLibrary,
    showAutomations,
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
    (item) => !FOCUS_IDS.has(item.id) && item.surface?.kind !== "custom",
  );
  const focus = layout.primary.filter((item) => FOCUS_IDS.has(item.id));
  const custom = layout.primary.filter(
    (item) => item.kind === "surface" && item.surface.kind === "custom",
  );
  const sections: { id: RailSectionId; label: string; items: LifeRailItem[] }[] = [];
  if (talk.length) sections.push({ id: "channels", label: "Channels", items: talk });
  if (focus.length) sections.push({ id: "focus", label: "Focus", items: focus });
  if (layout.showLibrary || layout.showAutomations) {
    const items: LifeRailItem[] = [];
    if (layout.showLibrary) {
      items.push({ kind: "surface", id: "library", surface: libraryRailSurface() });
    }
    if (layout.showAutomations) {
      items.push({
        kind: "surface",
        id: "automations",
        surface: automationsRailSurface(),
      });
    }
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
