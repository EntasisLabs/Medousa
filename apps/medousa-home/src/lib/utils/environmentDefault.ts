import type { EnvironmentSpec, SurfaceDef } from "$lib/types/environment";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";

const DEFAULT_PROFILE_ID = "personal";
const DEFAULT_PRESET_ID = "default";

function defaultShellChrome() {
  return {
    mobile: {
      defaultHome: "home",
      askEntry: "inline" as const,
      tabBar: "full" as const,
    },
    desktop: null,
  };
}

function defaultSurfaces() {
  const builtin: Array<{
    id: string;
    label: string;
    icon: string;
    builtinId: string;
    mobileTab?: string;
  }> = [
    { id: "home", label: "Home", icon: "home", builtinId: "home", mobileTab: "home" },
    { id: "chat", label: "Chat", icon: "message-circle", builtinId: "chat", mobileTab: "chat" },
    { id: "peers", label: "Peers", icon: "users", builtinId: "peers" },
    { id: "work", label: "Work", icon: "layout-grid", builtinId: "work" },
    { id: "code", label: "Code", icon: "code-2", builtinId: "code" },
    { id: "library", label: "Workspace", icon: "notebook-text", builtinId: "library" },
    { id: "notes", label: "Notes", icon: "notebook-text", builtinId: "notes", mobileTab: "notes" },
    { id: "files", label: "Files", icon: "folder", builtinId: "files" },
    { id: "artifacts", label: "Artifacts", icon: "sparkles", builtinId: "artifacts" },
    { id: "calendar", label: "Calendar", icon: "calendar-days", builtinId: "calendar" },
    { id: "web", label: "Web", icon: "globe", builtinId: "web", mobileTab: "web" },
    { id: "map", label: "Map", icon: "compass", builtinId: "map" },
    { id: "workshop", label: "Capabilities", icon: "zap", builtinId: "workshop" },
    { id: "automations", label: "Automations", icon: "zap", builtinId: "automations" },
    { id: "messaging", label: "Messaging", icon: "radio", builtinId: "messaging" },
    {
      id: SAFETY_SURFACE_RUNTIME,
      label: "Runtime",
      icon: "activity",
      builtinId: SAFETY_SURFACE_RUNTIME,
    },
    {
      id: SAFETY_SURFACE_SETTINGS,
      label: "Settings",
      icon: "settings",
      builtinId: SAFETY_SURFACE_SETTINGS,
    },
  ];

  return builtin.map((entry) => ({
    id: entry.id,
    label: entry.label,
    icon: entry.icon,
    kind: "builtin" as const,
    builtinId: entry.builtinId,
    layout: "single" as const,
    slots: [],
    mobileTab: entry.mobileTab ?? null,
  }));
}

function codeSurfaceDef(): SurfaceDef {
  return (
    defaultSurfaces().find((surface) => surface.id === "code") ?? {
      id: "code",
      label: "Code",
      icon: "code-2",
      kind: "builtin",
      builtinId: "code",
      layout: "single",
      slots: [],
      mobileTab: null,
    }
  );
}

function placeCodeAfterWork(surfaceIds: string[]): string[] {
  if (surfaceIds.includes("code")) return surfaceIds;
  const next = [...surfaceIds];
  const workAt = next.indexOf("work");
  if (workAt >= 0) {
    next.splice(workAt + 1, 0, "code");
    return next;
  }
  const libraryAt = next.indexOf("library");
  if (libraryAt >= 0) {
    next.splice(libraryAt, 0, "code");
    return next;
  }
  next.push("code");
  return next;
}

/** Ensure Code has a first-class destination on older saved layouts. */
export function ensureCodeSurfaceInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  const hasCode = spec.surfaces.some((surface) => surface.id === "code");
  let surfaces = [...spec.surfaces];
  if (!hasCode) {
    const workAt = surfaces.findIndex((surface) => surface.id === "work");
    const libraryAt = surfaces.findIndex((surface) => surface.id === "library");
    const insertAt = workAt >= 0 ? workAt + 1 : libraryAt >= 0 ? libraryAt : surfaces.length;
    surfaces.splice(insertAt, 0, codeSurfaceDef());
  }

  // Only seed preset membership while introducing the SurfaceDef. Once the
  // definition exists, absence from a preset is an intentional nav choice.
  const layoutPresets = hasCode
    ? (spec.layoutPresets ?? [])
    : (spec.layoutPresets ?? []).map((preset) => ({
        ...preset,
        surfaces: placeCodeAfterWork(preset.surfaces),
      }));
  const surfacesChanged =
    surfaces.length !== spec.surfaces.length ||
    surfaces.some((surface, index) => surface.id !== spec.surfaces[index]?.id);
  const presetsChanged = (spec.layoutPresets ?? []).some((preset, index) => {
    const next = layoutPresets[index];
    return !next || preset.surfaces.join("\0") !== next.surfaces.join("\0");
  });
  if (!surfacesChanged && !presetsChanged) return spec;
  return {
    ...spec,
    surfaces,
    layoutPresets: layoutPresets.length > 0 ? layoutPresets : spec.layoutPresets,
  };
}

const LIBRARY_SPLIT_IDS = ["notes", "files", "artifacts"] as const;

function librarySplitSurfaceDef(id: (typeof LIBRARY_SPLIT_IDS)[number]): SurfaceDef {
  const found = defaultSurfaces().find((surface) => surface.id === id);
  if (found) return found;
  const fallback: Record<(typeof LIBRARY_SPLIT_IDS)[number], SurfaceDef> = {
    notes: {
      id: "notes",
      label: "Notes",
      icon: "notebook-text",
      kind: "builtin",
      builtinId: "notes",
      layout: "single",
      slots: [],
      mobileTab: "notes",
    },
    files: {
      id: "files",
      label: "Files",
      icon: "folder",
      kind: "builtin",
      builtinId: "files",
      layout: "single",
      slots: [],
      mobileTab: null,
    },
    artifacts: {
      id: "artifacts",
      label: "Artifacts",
      icon: "sparkles",
      kind: "builtin",
      builtinId: "artifacts",
      layout: "single",
      slots: [],
      mobileTab: null,
    },
  };
  return fallback[id];
}

/** Replace a single `library` slot with notes/files/artifacts, preserving order. */
function replaceLibraryWithSplit(surfaceIds: string[]): string[] {
  if (!surfaceIds.includes("library")) return surfaceIds;
  const next: string[] = [];
  for (const id of surfaceIds) {
    if (id === "library") {
      for (const splitId of LIBRARY_SPLIT_IDS) {
        if (!next.includes(splitId)) next.push(splitId);
      }
      continue;
    }
    next.push(id);
  }
  return next;
}

/**
 * Promote Library's three explorer modes to first-class destinations on older
 * saved layouts. Keeps `library` as the internal LME tab host.
 */
export function ensureLibrarySplitInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  let surfaces = [...spec.surfaces];
  for (const id of LIBRARY_SPLIT_IDS) {
    if (!surfaces.some((surface) => surface.id === id)) {
      const libraryAt = surfaces.findIndex((surface) => surface.id === "library");
      const insertAt = libraryAt >= 0 ? libraryAt + 1 : surfaces.length;
      surfaces.splice(insertAt, 0, librarySplitSurfaceDef(id));
    }
  }

  const layoutPresets = (spec.layoutPresets ?? []).map((preset) => ({
    ...preset,
    surfaces: replaceLibraryWithSplit(preset.surfaces),
  }));

  const surfacesChanged =
    surfaces.length !== spec.surfaces.length ||
    surfaces.some((surface, index) => surface.id !== spec.surfaces[index]?.id);
  const presetsChanged = (spec.layoutPresets ?? []).some((preset, index) => {
    const next = layoutPresets[index];
    if (!next) return true;
    if (preset.surfaces.length !== next.surfaces.length) return true;
    return preset.surfaces.some((id, i) => id !== next.surfaces[i]);
  });

  if (!surfacesChanged && !presetsChanged) return spec;
  return {
    ...spec,
    surfaces,
    layoutPresets: layoutPresets.length > 0 ? layoutPresets : spec.layoutPresets,
  };
}

function peersSurfaceDef(): SurfaceDef {
  return (
    defaultSurfaces().find((surface) => surface.id === "peers") ?? {
      id: "peers",
      label: "Peers",
      icon: "users",
      kind: "builtin",
      builtinId: "peers",
      layout: "single",
      slots: [],
      mobileTab: null,
    }
  );
}

/** Insert peers next to chat when missing from a preset surface list. */
function ensurePeersInPresetSurfaces(surfaceIds: string[]): string[] {
  if (surfaceIds.includes("peers")) return surfaceIds;
  const next = [...surfaceIds];
  const chatAt = next.indexOf("chat");
  if (chatAt >= 0) {
    next.splice(chatAt + 1, 0, "peers");
    return next;
  }
  const messagingIndex = next.indexOf("messaging");
  if (messagingIndex >= 0) {
    next.splice(messagingIndex, 0, "peers");
    return next;
  }
  next.push("peers");
  return next;
}

/** Ensure Peers exists on older specs. Preserves operator-chosen rail order. */
export function ensurePeersSurfaceInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  const hasPeers = spec.surfaces.some((surface) => surface.id === "peers");
  let surfaces = [...spec.surfaces];

  if (!hasPeers) {
    const chatIndex = surfaces.findIndex((surface) => surface.id === "chat");
    const insertAt = chatIndex >= 0 ? chatIndex + 1 : surfaces.length;
    surfaces.splice(insertAt, 0, peersSurfaceDef());
  }

  const layoutPresets = hasPeers
    ? (spec.layoutPresets ?? [])
    : (spec.layoutPresets ?? []).map((preset) => ({
        ...preset,
        surfaces: ensurePeersInPresetSurfaces(preset.surfaces),
      }));

  const surfacesChanged =
    surfaces.length !== spec.surfaces.length ||
    surfaces.some((surface, index) => surface.id !== spec.surfaces[index]?.id);
  const presetsChanged = (spec.layoutPresets ?? []).some((preset, index) => {
    const next = layoutPresets[index];
    if (!next) return true;
    if (preset.surfaces.length !== next.surfaces.length) return true;
    return preset.surfaces.some((id, i) => id !== next.surfaces[i]);
  });

  if (!surfacesChanged && !presetsChanged) {
    return spec;
  }

  return {
    ...spec,
    surfaces,
    layoutPresets: layoutPresets.length > 0 ? layoutPresets : spec.layoutPresets,
  };
}

function calendarSurfaceDef(): SurfaceDef {
  return (
    defaultSurfaces().find((surface) => surface.id === "calendar") ?? {
      id: "calendar",
      label: "Calendar",
      icon: "calendar-days",
      kind: "builtin",
      builtinId: "calendar",
      layout: "single",
      slots: [],
      mobileTab: null,
    }
  );
}

function placeCalendarAfterLibrary(surfaceIds: string[]): string[] {
  if (surfaceIds.includes("calendar")) return surfaceIds;
  const next = [...surfaceIds];
  const libraryAt = next.indexOf("library");
  if (libraryAt >= 0) {
    next.splice(libraryAt + 1, 0, "calendar");
    return next;
  }
  const webAt = next.indexOf("web");
  if (webAt >= 0) {
    next.splice(webAt, 0, "calendar");
    return next;
  }
  next.push("calendar");
  return next;
}

/** Ensure Calendar exists after Library in the rail. */
export function ensureCalendarSurfaceInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  const hasCalendar = spec.surfaces.some((surface) => surface.id === "calendar");
  let surfaces = [...spec.surfaces];

  if (!hasCalendar) {
    const libraryIndex = surfaces.findIndex((surface) => surface.id === "library");
    const insertAt = libraryIndex >= 0 ? libraryIndex + 1 : surfaces.length;
    surfaces.splice(insertAt, 0, calendarSurfaceDef());
  }

  const layoutPresets = hasCalendar
    ? (spec.layoutPresets ?? [])
    : (spec.layoutPresets ?? []).map((preset) => ({
        ...preset,
        surfaces: placeCalendarAfterLibrary(preset.surfaces),
      }));

  const surfacesChanged =
    surfaces.length !== spec.surfaces.length ||
    surfaces.some((surface, index) => surface.id !== spec.surfaces[index]?.id);
  const presetsChanged = (spec.layoutPresets ?? []).some((preset, index) => {
    const next = layoutPresets[index];
    if (!next) return true;
    if (preset.surfaces.length !== next.surfaces.length) return true;
    return preset.surfaces.some((id, i) => id !== next.surfaces[i]);
  });

  if (!surfacesChanged && !presetsChanged) {
    return spec;
  }

  return {
    ...spec,
    surfaces,
    layoutPresets: layoutPresets.length > 0 ? layoutPresets : spec.layoutPresets,
  };
}

function mapSurfaceDef(): SurfaceDef {
  return (
    defaultSurfaces().find((surface) => surface.id === "map") ?? {
      id: "map",
      label: "Map",
      icon: "compass",
      kind: "builtin",
      builtinId: "map",
      layout: "single",
      slots: [],
      mobileTab: null,
    }
  );
}

function replaceRetiredContextSurface(
  surfaceIds: string[],
  seedMissingMap: boolean,
): string[] {
  const hadContext = surfaceIds.includes("context");
  const withoutContext = surfaceIds.filter((id) => id !== "context");
  if (withoutContext.includes("map")) return withoutContext;
  if (!hadContext && !seedMissingMap) return withoutContext;
  const next = [...withoutContext];
  const libraryAt = next.indexOf("library");
  if (libraryAt >= 0) {
    next.splice(libraryAt + 1, 0, "map");
    return next;
  }
  const webAt = next.indexOf("web");
  if (webAt >= 0) {
    next.splice(webAt + 1, 0, "map");
    return next;
  }
  next.push("map");
  return next;
}

/** Ensure Map exists; strip retired Context surface from specs/presets. */
export function ensureMapSurfaceInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  let surfaces = spec.surfaces.filter((surface) => surface.id !== "context");
  const hasMap = surfaces.some((surface) => surface.id === "map");

  if (!hasMap) {
    const libraryIndex = surfaces.findIndex((surface) => surface.id === "library");
    const insertAt = libraryIndex >= 0 ? libraryIndex + 1 : surfaces.length;
    surfaces = [...surfaces];
    surfaces.splice(insertAt, 0, mapSurfaceDef());
  }

  const layoutPresets = (spec.layoutPresets ?? []).map((preset) => ({
    ...preset,
    surfaces: replaceRetiredContextSurface(preset.surfaces, !hasMap),
  }));

  const surfacesChanged =
    surfaces.length !== spec.surfaces.length ||
    surfaces.some((surface, index) => surface.id !== spec.surfaces[index]?.id);
  const presetsChanged = (spec.layoutPresets ?? []).some((preset, index) => {
    const next = layoutPresets[index];
    if (!next) return true;
    if (preset.surfaces.length !== next.surfaces.length) return true;
    return preset.surfaces.some((id, i) => id !== next.surfaces[i]);
  });

  if (!surfacesChanged && !presetsChanged) {
    return spec;
  }

  return {
    ...spec,
    surfaces,
    layoutPresets: layoutPresets.length > 0 ? layoutPresets : spec.layoutPresets,
  };
}

export function defaultEnvironmentSpec(
  profileId = DEFAULT_PROFILE_ID,
): EnvironmentSpec {
  const now = new Date().toISOString();
  const surfaces = defaultSurfaces();
  return {
    version: 1,
    profileId,
    surfaces,
    components: [],
    layoutPresets: [
      {
        id: DEFAULT_PRESET_ID,
        label: "Default",
        active: true,
        // `library` stays defined as the LME host but is not a rail door.
        surfaces: surfaces
          .map((surface) => surface.id)
          .filter((id) => id !== "library"),
        shellChrome: defaultShellChrome(),
      },
      {
        id: "focus",
        label: "Focus",
        active: false,
        surfaces: [
          "chat",
          "peers",
          "work",
          "code",
          "notes",
          "web",
          "files",
          "artifacts",
          "map",
          SAFETY_SURFACE_SETTINGS,
          SAFETY_SURFACE_RUNTIME,
        ],
        shellChrome: defaultShellChrome(),
      },
    ],
    activePresetId: DEFAULT_PRESET_ID,
    shellChrome: defaultShellChrome(),
    theme: null,
    updatedAt: now,
    updatedBy: "system",
  };
}
