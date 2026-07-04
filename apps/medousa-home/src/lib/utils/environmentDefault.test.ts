import { describe, expect, it } from "vitest";
import {
  defaultEnvironmentSpec,
  ensurePeersSurfaceInSpec,
} from "$lib/utils/environmentDefault";

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

  it("moves buried peers next to chat", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter((id) => id !== "peers");
    preset.surfaces.push("peers");

    const next = ensurePeersSurfaceInSpec(spec);
    const chatAt = next.layoutPresets![0]!.surfaces.indexOf("chat");
    const peersAt = next.layoutPresets![0]!.surfaces.indexOf("peers");
    expect(peersAt).toBe(chatAt + 1);
  });
});
