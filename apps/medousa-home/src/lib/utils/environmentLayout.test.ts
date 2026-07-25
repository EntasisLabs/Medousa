import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import {
  activeLayoutPreset,
  activePresetSurfaceIds,
  addLayoutPresetFromActive,
  activateLayoutPreset,
  isSurfaceNavVisible,
  moveSurfaceInActivePreset,
  removeLayoutPreset,
  reorderPrimarySurfaceInActivePreset,
  reorderSurfaceInActivePreset,
  setActiveLayoutTheme,
  setSurfaceNavVisible,
} from "$lib/utils/environmentLayout";
import { primaryRailSurfaceIds } from "$lib/utils/lifeRailSections";

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

describe("environmentLayout reorder", () => {
  it("moves a destination up and down inside the active preset", () => {
    const spec = defaultEnvironmentSpec();
    const before = activePresetSurfaceIds(spec);
    const webAt = before.indexOf("web");
    expect(webAt).toBeGreaterThan(0);

    moveSurfaceInActivePreset(spec, "web", -1);
    const afterUp = activePresetSurfaceIds(spec);
    expect(afterUp.indexOf("web")).toBe(webAt - 1);
    expect(afterUp).toContain("settings");
    expect(afterUp).toContain("runtime");

    moveSurfaceInActivePreset(spec, "web", 1);
    expect(activePresetSurfaceIds(spec).indexOf("web")).toBe(webAt);
  });

  it("no-ops at the ends of the movable range", () => {
    const spec = defaultEnvironmentSpec();
    const movable = activePresetSurfaceIds(spec).filter(
      (id) => id !== "settings" && id !== "runtime" && id !== "home",
    );
    const first = movable[0]!;
    const last = movable[movable.length - 1]!;
    const before = activePresetSurfaceIds(spec);

    moveSurfaceInActivePreset(spec, first, -1);
    expect(activePresetSurfaceIds(spec)).toEqual(before);

    moveSurfaceInActivePreset(spec, last, 1);
    expect(activePresetSurfaceIds(spec)).toEqual(before);
  });

  it("reorders a destination before another (or to the end)", () => {
    const spec = defaultEnvironmentSpec();
    const before = activePresetSurfaceIds(spec);
    expect(before.indexOf("library")).toBeGreaterThan(before.indexOf("chat"));

    reorderSurfaceInActivePreset(spec, "library", "chat");
    const after = activePresetSurfaceIds(spec);
    expect(after.indexOf("library")).toBeLessThan(after.indexOf("chat"));
    expect(after).toContain("settings");
    expect(after).toContain("runtime");

    reorderSurfaceInActivePreset(spec, "library", null);
    const atEnd = activePresetSurfaceIds(spec);
    const libraryAt = atEnd.indexOf("library");
    const settingsAt = atEnd.indexOf("settings");
    expect(libraryAt).toBeGreaterThan(-1);
    expect(libraryAt).toBeLessThan(settingsAt);
  });

  it("lets Automations reorder independently of Library", () => {
    const spec = defaultEnvironmentSpec();
    reorderSurfaceInActivePreset(spec, "automations", "chat");
    const ids = activePresetSurfaceIds(spec);
    expect(ids.indexOf("automations")).toBeLessThan(ids.indexOf("chat"));
    expect(ids.indexOf("automations")).not.toBe(ids.indexOf("library") + 1);
  });

  it("reorders by primary-rail index without collapsing dock surfaces", () => {
    const spec = defaultEnvironmentSpec();
    const preset = activeLayoutPreset(spec);
    expect(preset).toBeTruthy();
    // Interleave a dock surface between primary doors.
    preset!.surfaces = [
      "chat",
      "peers",
      "context",
      "work",
      "library",
      "automations",
      "settings",
      "runtime",
    ];

    reorderPrimarySurfaceInActivePreset(spec, "work", 0);
    expect(primaryRailSurfaceIds(activePresetSurfaceIds(spec))).toEqual([
      "work",
      "chat",
      "peers",
      "library",
      "automations",
    ]);
    expect(activePresetSurfaceIds(spec).indexOf("context")).toBeGreaterThan(
      activePresetSurfaceIds(spec).indexOf("peers"),
    );

    reorderPrimarySurfaceInActivePreset(spec, "work", 4);
    expect(primaryRailSurfaceIds(activePresetSurfaceIds(spec))).toEqual([
      "chat",
      "peers",
      "library",
      "automations",
      "work",
    ]);
  });
});

describe("environmentLayout theme", () => {
  it("stamps theme on the env and active layout", () => {
    const spec = defaultEnvironmentSpec();
    setActiveLayoutTheme(spec, { colorThemeId: "ember" });
    expect(spec.theme?.colorThemeId).toBe("ember");
    expect(activeLayoutPreset(spec)?.theme?.colorThemeId).toBe("ember");
  });

  it("applies a layout theme when activating", () => {
    const spec = defaultEnvironmentSpec();
    setActiveLayoutTheme(spec, { colorThemeId: "ember" });
    const writingId = addLayoutPresetFromActive(spec, { label: "Writing" });
    expect(activeLayoutPreset(spec)?.theme?.colorThemeId).toBe("ember");

    activateLayoutPreset(spec, "default");
    expect(spec.theme?.colorThemeId).toBe("ember");

    const focus = spec.layoutPresets?.find((preset) => preset.id === "focus");
    expect(focus).toBeTruthy();
    focus!.theme = { colorThemeId: "nord" };
    activateLayoutPreset(spec, "focus");
    expect(spec.theme?.colorThemeId).toBe("nord");

    activateLayoutPreset(spec, writingId);
    expect(spec.theme?.colorThemeId).toBe("ember");
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
