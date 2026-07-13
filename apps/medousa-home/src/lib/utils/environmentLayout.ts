import type { EnvironmentSpec } from "$lib/types/environment";
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
  { label: "Connect", surfaceIds: ["messaging"] },
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
  });
  spec.activePresetId = id;
  if (active.shellChrome) {
    spec.shellChrome = structuredClone(active.shellChrome);
  }
  return id;
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
