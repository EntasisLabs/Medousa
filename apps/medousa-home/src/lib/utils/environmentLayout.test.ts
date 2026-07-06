import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import {
  activePresetSurfaceIds,
  addLayoutPresetFromActive,
  activateLayoutPreset,
  isSurfaceNavVisible,
  removeLayoutPreset,
  setSurfaceNavVisible,
} from "$lib/utils/environmentLayout";

describe("environmentLayout nav visibility", () => {
  it("hides and restores a builtin surface on the active preset", () => {
    const spec = defaultEnvironmentSpec();
    expect(isSurfaceNavVisible(spec, "web")).toBe(true);

    setSurfaceNavVisible(spec, "web", false);
    expect(isSurfaceNavVisible(spec, "web")).toBe(false);
    expect(activePresetSurfaceIds(spec)).not.toContain("web");

    setSurfaceNavVisible(spec, "web", true);
    expect(isSurfaceNavVisible(spec, "web")).toBe(true);
  });

  it("keeps safety surfaces when hiding native views", () => {
    const spec = defaultEnvironmentSpec();
    setSurfaceNavVisible(spec, "chat", false);
    setSurfaceNavVisible(spec, "work", false);

    expect(activePresetSurfaceIds(spec)).toContain("settings");
    expect(activePresetSurfaceIds(spec)).toContain("runtime");
  });

  it("rejects toggling safety surfaces", () => {
    const spec = defaultEnvironmentSpec();
    expect(() => setSurfaceNavVisible(spec, "settings", false)).toThrow(/cannot be hidden/i);
  });
});

describe("environmentLayout presets", () => {
  it("clones the active preset into a new layout and activates it", () => {
    const spec = defaultEnvironmentSpec();
    setSurfaceNavVisible(spec, "web", false);
    const id = addLayoutPresetFromActive(spec, { label: "Writing mode" });

    expect(id).toBe("writing-mode");
    const created = spec.layoutPresets?.find((preset) => preset.id === id);
    expect(created?.active).toBe(true);
    expect(created?.surfaces).toEqual(activePresetSurfaceIds(spec));
    expect(created?.surfaces).not.toContain("web");
    expect(spec.activePresetId).toBe(id);
  });

  it("removes custom presets but not built-ins", () => {
    const spec = defaultEnvironmentSpec();
    const id = addLayoutPresetFromActive(spec, { label: "Temp" });
    activateLayoutPreset(spec, "default");

    removeLayoutPreset(spec, id);
    expect(spec.layoutPresets?.some((preset) => preset.id === id)).toBe(false);
    expect(() => removeLayoutPreset(spec, "focus")).toThrow(/built-in/i);
  });

  it("rejects deleting the active preset", () => {
    const spec = defaultEnvironmentSpec();
    const id = addLayoutPresetFromActive(spec, { label: "Active temp" });
    expect(() => removeLayoutPreset(spec, id)).toThrow(/switch to another/i);
  });
});

describe("updateCustomSurfaceInSpec", () => {
  it("updates label and icon on custom surfaces only", async () => {
    const { defaultEnvironmentSpec } = await import("$lib/utils/environmentDefault");
    const { addCustomSurfaceToSpec, updateCustomSurfaceInSpec } = await import(
      "$lib/utils/environmentCanvasOps"
    );
    const spec = defaultEnvironmentSpec();
    addCustomSurfaceToSpec(spec, {
      id: "studio",
      label: "Studio",
      icon: "sparkles",
    });

    updateCustomSurfaceInSpec(spec, "studio", { label: "Writing studio", icon: "pen-line" });
    const surface = spec.surfaces.find((entry) => entry.id === "studio");
    expect(surface?.label).toBe("Writing studio");
    expect(surface?.icon).toBe("pen-line");
    expect(() => updateCustomSurfaceInSpec(spec, "home", { label: "Nope" })).toThrow(/custom/i);
  });
});
