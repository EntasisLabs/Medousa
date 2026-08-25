import { describe, expect, it } from "vitest";
import {
  defaultEnvironmentSpec,
  ensureCalendarSurfaceInSpec,
  ensureCodeSurfaceInSpec,
  ensureMapSurfaceInSpec,
  ensurePeersSurfaceInSpec,
} from "$lib/utils/environmentDefault";

describe("ensureMapSurfaceInSpec", () => {
  it("keeps a single map surface and no context on the default spec", () => {
    const spec = defaultEnvironmentSpec();
    const next = ensureMapSurfaceInSpec(spec);
    expect(next.surfaces.filter((surface) => surface.id === "map")).toHaveLength(1);
    expect(next.surfaces.some((surface) => surface.id === "context")).toBe(false);
  });

  it("inserts map into older specs missing the surface", () => {
    const spec = defaultEnvironmentSpec();
    spec.surfaces = spec.surfaces.filter((surface) => surface.id !== "map");
    for (const preset of spec.layoutPresets ?? []) {
      preset.surfaces = preset.surfaces.filter((id) => id !== "map");
    }

    const next = ensureMapSurfaceInSpec(spec);
    expect(next.surfaces.some((surface) => surface.id === "map")).toBe(true);
    const map = next.surfaces.find((surface) => surface.id === "map");
    expect(map?.label).toBe("Map");
    expect(map?.icon).toBe("compass");
    expect(next.layoutPresets?.[0]?.surfaces).toContain("map");
  });

  it("strips retired context from older specs", () => {
    const spec = defaultEnvironmentSpec();
    const webAt = spec.surfaces.findIndex((surface) => surface.id === "web");
    spec.surfaces.splice(webAt + 1, 0, {
      id: "context",
      label: "Context",
      icon: "orbit",
      kind: "builtin",
      builtinId: "context",
      layout: "single",
      slots: [],
      mobileTab: null,
    });
    for (const preset of spec.layoutPresets ?? []) {
      const idx = preset.surfaces.indexOf("web");
      if (idx >= 0) preset.surfaces.splice(idx + 1, 0, "context");
    }

    const next = ensureMapSurfaceInSpec(spec);
    expect(next.surfaces.some((surface) => surface.id === "context")).toBe(false);
    expect(next.layoutPresets?.[0]?.surfaces).not.toContain("context");
    expect(next.surfaces.some((surface) => surface.id === "map")).toBe(true);
  });

  it("preserves an intentionally hidden map destination", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "map");

    expect(ensureMapSurfaceInSpec(spec)).toBe(spec);
    expect(preset.surfaces).not.toContain("map");
  });
});

describe("ensureCodeSurfaceInSpec", () => {
  it("adds Code after Work to older specs and every saved layout", () => {
    const spec = defaultEnvironmentSpec();
    spec.surfaces = spec.surfaces.filter((surface) => surface.id !== "code");
    for (const preset of spec.layoutPresets ?? []) {
      preset.surfaces = preset.surfaces.filter((id) => id !== "code");
    }

    const next = ensureCodeSurfaceInSpec(spec);
    const workAt = next.surfaces.findIndex((surface) => surface.id === "work");
    expect(next.surfaces[workAt + 1]?.id).toBe("code");
    expect(next.surfaces[workAt + 1]?.icon).toBe("code-2");
    for (const preset of next.layoutPresets ?? []) {
      const presetWorkAt = preset.surfaces.indexOf("work");
      expect(preset.surfaces[presetWorkAt + 1]).toBe("code");
    }
  });

  it("preserves an operator-chosen Code position", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "code");
    preset.surfaces.push("code");

    expect(ensureCodeSurfaceInSpec(spec)).toBe(spec);
    expect(spec.layoutPresets![0]!.surfaces.at(-1)).toBe("code");
  });

  it("preserves an intentionally hidden Code destination", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "code");

    expect(ensureCodeSurfaceInSpec(spec)).toBe(spec);
    expect(preset.surfaces).not.toContain("code");
  });
});

describe("ensurePeersSurfaceInSpec", () => {
  it("is a no-op when peers already sits after chat", () => {
    const spec = defaultEnvironmentSpec();
    const next = ensurePeersSurfaceInSpec(spec);
    expect(next.surfaces.filter((surface) => surface.id === "peers")).toHaveLength(1);
    expect(next).toBe(spec);
  });

  it("inserts peers into older specs missing the surface", () => {
    const spec = defaultEnvironmentSpec();
    spec.surfaces = spec.surfaces.filter((surface) => surface.id !== "peers");
    for (const preset of spec.layoutPresets ?? []) {
      preset.surfaces = preset.surfaces.filter((id) => id !== "peers");
    }

    const next = ensurePeersSurfaceInSpec(spec);
    expect(next.surfaces.some((surface) => surface.id === "peers")).toBe(true);
    const peers = next.surfaces.find((surface) => surface.id === "peers");
    expect(peers?.label).toBe("Peers");
    expect(peers?.icon).toBe("users");
    expect(next.layoutPresets?.[0]?.surfaces).toContain("peers");
    const chatAt = next.layoutPresets?.[0]?.surfaces.indexOf("chat") ?? -1;
    const peersAt = next.layoutPresets?.[0]?.surfaces.indexOf("peers") ?? -1;
    expect(peersAt).toBe(chatAt + 1);
  });

  it("preserves a custom peers position once present", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "peers");
    preset.surfaces.push("peers");

    const next = ensurePeersSurfaceInSpec(spec);
    expect(next).toBe(spec);
    expect(next.layoutPresets![0]!.surfaces.at(-1)).toBe("peers");
  });

  it("preserves an intentionally hidden Peers destination", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "peers");

    expect(ensurePeersSurfaceInSpec(spec)).toBe(spec);
    expect(preset.surfaces).not.toContain("peers");
  });
});

describe("ensureCalendarSurfaceInSpec", () => {
  it("adds Calendar to presets only while introducing the older missing surface", () => {
    const spec = defaultEnvironmentSpec();
    spec.surfaces = spec.surfaces.filter((surface) => surface.id !== "calendar");
    for (const preset of spec.layoutPresets ?? []) {
      preset.surfaces = preset.surfaces.filter((id) => id !== "calendar");
    }

    const next = ensureCalendarSurfaceInSpec(spec);
    expect(next.surfaces.some((surface) => surface.id === "calendar")).toBe(true);
    expect(next.layoutPresets?.[0]?.surfaces).toContain("calendar");
  });

  it("preserves an intentionally hidden Calendar destination", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "calendar");

    expect(ensureCalendarSurfaceInSpec(spec)).toBe(spec);
    expect(preset.surfaces).not.toContain("calendar");
  });
});
