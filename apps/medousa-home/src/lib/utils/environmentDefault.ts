import type { EnvironmentSpec } from "$lib/types/environment";
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
    { id: "work", label: "Work", icon: "layout-grid", builtinId: "work" },
    { id: "library", label: "Library", icon: "book-open", builtinId: "library", mobileTab: "notes" },
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
        surfaces: ["chat", "work", "library", SAFETY_SURFACE_SETTINGS, SAFETY_SURFACE_RUNTIME],
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
