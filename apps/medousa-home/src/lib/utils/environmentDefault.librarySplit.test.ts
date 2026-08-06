import { describe, expect, it } from "vitest";
import {
  defaultEnvironmentSpec,
  ensureLibrarySplitInSpec,
} from "./environmentDefault";

describe("ensureLibrarySplitInSpec", () => {
  it("replaces library in presets with notes/files/artifacts in place", () => {
    const spec = defaultEnvironmentSpec();
    // Simulate a stale saved preset that still has a single library door.
    for (const preset of spec.layoutPresets ?? []) {
      preset.surfaces = preset.surfaces
        .filter((id) => id !== "notes" && id !== "files" && id !== "artifacts")
        .map((id) => id);
      const codeAt = preset.surfaces.indexOf("code");
      const insertAt = codeAt >= 0 ? codeAt + 1 : preset.surfaces.length;
      if (!preset.surfaces.includes("library")) {
        preset.surfaces.splice(insertAt, 0, "library");
      }
    }
    spec.surfaces = spec.surfaces.filter(
      (surface) =>
        surface.id !== "notes" &&
        surface.id !== "files" &&
        surface.id !== "artifacts",
    );

    const migrated = ensureLibrarySplitInSpec(spec);
    const defaultPreset = migrated.layoutPresets?.find((p) => p.id === "default");
    expect(defaultPreset?.surfaces).toContain("notes");
    expect(defaultPreset?.surfaces).toContain("files");
    expect(defaultPreset?.surfaces).toContain("artifacts");
    expect(defaultPreset?.surfaces).not.toContain("library");

    const codeAt = defaultPreset!.surfaces.indexOf("code");
    expect(defaultPreset!.surfaces.indexOf("notes")).toBe(codeAt + 1);
    expect(defaultPreset!.surfaces.indexOf("files")).toBe(codeAt + 2);
    expect(defaultPreset!.surfaces.indexOf("artifacts")).toBe(codeAt + 3);

    expect(migrated.surfaces.some((s) => s.id === "notes")).toBe(true);
    expect(migrated.surfaces.some((s) => s.id === "files")).toBe(true);
    expect(migrated.surfaces.some((s) => s.id === "artifacts")).toBe(true);
    // Host surface stays for LME tabs.
    expect(migrated.surfaces.some((s) => s.id === "library")).toBe(true);
  });

  it("is a no-op when the split is already present", () => {
    const spec = defaultEnvironmentSpec();
    const again = ensureLibrarySplitInSpec(spec);
    expect(again).toBe(spec);
  });
});
