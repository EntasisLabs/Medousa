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
    { id: "library", label: "Workspace", icon: "book-open", builtinId: "library", mobileTab: "notes" },
    { id: "calendar", label: "Calendar", icon: "calendar-days", builtinId: "calendar" },
    { id: "web", label: "Web", icon: "globe", builtinId: "web", mobileTab: "web" },
    { id: "context", label: "Context", icon: "orbit", builtinId: "context" },
    { id: "workshop", label: "Capabilities", icon: "zap", builtinId: "workshop" },
    { id: "automations", label: "Automations", icon: "calendar", builtinId: "automations" },
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

function placePeersAfterChat(surfaceIds: string[]): string[] {
  const withoutPeers = surfaceIds.filter((id) => id !== "peers");
  const chatAt = withoutPeers.indexOf("chat");
  if (chatAt >= 0) {
    withoutPeers.splice(chatAt + 1, 0, "peers");
    return withoutPeers;
  }
  const messagingIndex = withoutPeers.indexOf("messaging");
  if (messagingIndex >= 0) {
    withoutPeers.splice(messagingIndex, 0, "peers");
    return withoutPeers;
  }
  withoutPeers.push("peers");
  return withoutPeers;
}

/** Ensure Peers exists and sits next to Chat in the rail. */
export function ensurePeersSurfaceInSpec(spec: EnvironmentSpec): EnvironmentSpec {
  const hasPeers = spec.surfaces.some((surface) => surface.id === "peers");
  let surfaces = [...spec.surfaces];

  if (!hasPeers) {
    const chatIndex = surfaces.findIndex((surface) => surface.id === "chat");
    const insertAt = chatIndex >= 0 ? chatIndex + 1 : surfaces.length;
    surfaces.splice(insertAt, 0, peersSurfaceDef());
  } else {
    // Keep surface list order aligned with rail preference.
    const peers = surfaces.find((surface) => surface.id === "peers")!;
    const withoutPeers = surfaces.filter((surface) => surface.id !== "peers");
    const chatIndex = withoutPeers.findIndex((surface) => surface.id === "chat");
    const insertAt = chatIndex >= 0 ? chatIndex + 1 : withoutPeers.length;
    withoutPeers.splice(insertAt, 0, peers);
    surfaces = withoutPeers;
  }

  const layoutPresets = (spec.layoutPresets ?? []).map((preset) => ({
    ...preset,
    surfaces: placePeersAfterChat(preset.surfaces),
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

  const layoutPresets = (spec.layoutPresets ?? []).map((preset) => ({
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
        surfaces: surfaces.map((surface) => surface.id),
        shellChrome: defaultShellChrome(),
      },
      {
        id: "focus",
        label: "Focus",
        active: false,
        surfaces: ["chat", "peers", "work", "library", SAFETY_SURFACE_SETTINGS, SAFETY_SURFACE_RUNTIME],
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
