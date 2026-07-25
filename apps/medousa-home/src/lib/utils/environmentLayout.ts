import type { EnvironmentSpec, EnvironmentTheme } from "$lib/types/environment";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";

export const SAFETY_PRESET_SURFACE_IDS = [
  SAFETY_SURFACE_SETTINGS,
  SAFETY_SURFACE_RUNTIME,
] as const;

/** Surfaces that always stay available and cannot be toggled off in Settings. */
export const NON_TOGGLEABLE_NAV_SURFACE_IDS = new Set([
  "home",
  SAFETY_SURFACE_SETTINGS,
  SAFETY_SURFACE_RUNTIME,
]);

export const NAV_DESTINATION_GROUPS: Array<{ label: string; surfaceIds: string[] }> = [
  { label: "Life", surfaceIds: ["chat", "peers", "work", "library", "web", "context", "calendar"] },
  { label: "Workshop", surfaceIds: ["workshop", "automations"] },
];

/** Switch active layout preset on an in-memory spec (mirrors daemon helper). */
export function activateLayoutPreset(spec: EnvironmentSpec, presetId: string): void {
  const presets = spec.layoutPresets ?? [];
  const preset = presets.find((entry) => entry.id === presetId);
  if (!preset) {
    throw new Error(`Unknown layout preset '${presetId}'`);
  }
  for (const entry of presets) {
    entry.active = entry.id === presetId;
  }
  spec.activePresetId = presetId;
  if (preset.shellChrome) {
    spec.shellChrome = preset.shellChrome;
  }
  // Promote (or clear) so the previous layout's palette does not stick.
  spec.theme = preset.theme ? structuredClone(preset.theme) : null;
}

export function activeLayoutPreset(spec: EnvironmentSpec) {
  const presets = spec.layoutPresets ?? [];
  return (
    presets.find((entry) => entry.active) ??
    presets.find((entry) => entry.id === spec.activePresetId) ??
    null
  );
}

export function activePresetSurfaceIds(spec: EnvironmentSpec): string[] {
  const preset = activeLayoutPreset(spec);
  if (preset) return [...preset.surfaces];
  return spec.surfaces.map((surface) => surface.id);
}

export function isSurfaceNavVisible(spec: EnvironmentSpec, surfaceId: string): boolean {
  return activePresetSurfaceIds(spec).includes(surfaceId);
}

export function isNavDestinationToggleable(surfaceId: string): boolean {
  return !NON_TOGGLEABLE_NAV_SURFACE_IDS.has(surfaceId);
}

export function setSurfaceNavVisible(
  spec: EnvironmentSpec,
  surfaceId: string,
  visible: boolean,
): void {
  if (!isNavDestinationToggleable(surfaceId)) {
    throw new Error(`Surface '${surfaceId}' cannot be hidden from nav.`);
  }
  if (!spec.surfaces.some((surface) => surface.id === surfaceId)) {
    throw new Error(`Unknown surface '${surfaceId}'.`);
  }

  const preset = activeLayoutPreset(spec);
  if (!preset) {
    throw new Error("No active layout preset.");
  }

  const next = [...preset.surfaces];
  const index = next.indexOf(surfaceId);
  if (visible) {
    if (index !== -1) return;
    const firstSafetyIndex = next.findIndex((id) =>
      (SAFETY_PRESET_SURFACE_IDS as readonly string[]).includes(id),
    );
    if (firstSafetyIndex === -1) {
      next.push(surfaceId);
    } else {
      next.splice(firstSafetyIndex, 0, surfaceId);
    }
  } else if (index !== -1) {
    next.splice(index, 1);
  }

  preset.surfaces = next;
}

/**
 * Move a destination within the active layout preset.
 * Only toggleable rail destinations move; home / safety stay pinned in place.
 */
export function moveSurfaceInActivePreset(
  spec: EnvironmentSpec,
  surfaceId: string,
  direction: -1 | 1,
): void {
  if (!isNavDestinationToggleable(surfaceId)) {
    throw new Error(`Surface '${surfaceId}' cannot be reordered in nav.`);
  }
  if (!spec.surfaces.some((surface) => surface.id === surfaceId)) {
    throw new Error(`Unknown surface '${surfaceId}'.`);
  }

  const preset = activeLayoutPreset(spec);
  if (!preset) {
    throw new Error("No active layout preset.");
  }

  const safety = new Set<string>(SAFETY_PRESET_SURFACE_IDS);
  const next = [...preset.surfaces];

  if (!next.includes(surfaceId)) {
    // Not in the active layout yet (e.g. Automations twin door) — place sensibly.
    const libraryAt = next.indexOf("library");
    if (surfaceId === "automations" && libraryAt >= 0) {
      next.splice(libraryAt + 1, 0, surfaceId);
    } else {
      const firstSafety = next.findIndex((id) => safety.has(id));
      if (firstSafety === -1) next.push(surfaceId);
      else next.splice(firstSafety, 0, surfaceId);
    }
  }

  const movableIndices: number[] = [];
  for (let i = 0; i < next.length; i++) {
    const id = next[i]!;
    if (safety.has(id) || !isNavDestinationToggleable(id)) continue;
    movableIndices.push(i);
  }

  const movablePos = movableIndices.findIndex((i) => next[i] === surfaceId);
  if (movablePos === -1) return;

  const targetPos = movablePos + direction;
  if (targetPos < 0 || targetPos >= movableIndices.length) {
    preset.surfaces = next;
    return;
  }

  const from = movableIndices[movablePos]!;
  const to = movableIndices[targetPos]!;
  const tmp = next[from]!;
  next[from] = next[to]!;
  next[to] = tmp;
  preset.surfaces = next;
}

export const BUILTIN_LAYOUT_PRESET_IDS = new Set(["default", "focus"]);

export function isBuiltinLayoutPreset(presetId: string): boolean {
  return BUILTIN_LAYOUT_PRESET_IDS.has(presetId);
}

export function uniqueLayoutPresetId(spec: EnvironmentSpec, base: string): string {
  const root = base
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48) || "layout";
  const presets = spec.layoutPresets ?? [];
  if (!presets.some((preset) => preset.id === root)) return root;
  let index = 2;
  while (presets.some((preset) => preset.id === `${root}-${index}`)) {
    index += 1;
  }
  return `${root}-${index}`;
}

/** Snapshot the active preset's nav membership into a new preset and activate it. */
export function addLayoutPresetFromActive(
  spec: EnvironmentSpec,
  input: { label: string; id?: string | null },
): string {
  const active = activeLayoutPreset(spec);
  if (!active) {
    throw new Error("No active layout preset.");
  }
  const label = input.label.trim();
  if (!label) {
    throw new Error("Layout name is required.");
  }
  const id = uniqueLayoutPresetId(spec, input.id?.trim() || label);
  if (!spec.layoutPresets) {
    spec.layoutPresets = [];
  }
  for (const preset of spec.layoutPresets) {
    preset.active = false;
  }
  spec.layoutPresets.push({
    id,
    label,
    active: true,
    surfaces: [...active.surfaces],
    shellChrome: active.shellChrome ? structuredClone(active.shellChrome) : null,
    theme: active.theme ? structuredClone(active.theme) : spec.theme ? structuredClone(spec.theme) : null,
  });
  spec.activePresetId = id;
  if (active.shellChrome) {
    spec.shellChrome = structuredClone(active.shellChrome);
  }
  const created = spec.layoutPresets.find((preset) => preset.id === id);
  if (created?.theme) {
    spec.theme = structuredClone(created.theme);
  }
  return id;
}

/** Stamp color theme onto the env + active layout (mirrors shell chrome dual-write). */
export function setActiveLayoutTheme(
  spec: EnvironmentSpec,
  theme: EnvironmentTheme,
): void {
  const next: EnvironmentTheme = {
    ...(spec.theme ?? {}),
    ...theme,
  };
  spec.theme = next;
  const active = activeLayoutPreset(spec);
  if (active) {
    active.theme = {
      ...(active.theme ?? {}),
      ...theme,
    };
  }
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
}

export function removeLayoutPreset(spec: EnvironmentSpec, presetId: string): void {
  const presets = spec.layoutPresets ?? [];
  if (presets.length <= 1) {
    throw new Error("At least one layout preset is required.");
  }
  const preset = presets.find((entry) => entry.id === presetId);
  if (!preset) {
    throw new Error(`Unknown layout preset '${presetId}'.`);
  }
  if (preset.active) {
    throw new Error("Switch to another layout before deleting this one.");
  }
  if (isBuiltinLayoutPreset(presetId)) {
    throw new Error("Built-in layouts cannot be deleted.");
  }
  spec.layoutPresets = presets.filter((entry) => entry.id !== presetId);
}
