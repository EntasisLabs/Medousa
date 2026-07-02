import type { EnvironmentSpec } from "$lib/types/environment";

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
