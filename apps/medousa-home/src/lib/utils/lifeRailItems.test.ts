import { describe, expect, it } from "vitest";
import type { SurfaceDef } from "$lib/types/environment";
import { LME_EXPLORER_MODES } from "./lmeExplorerModes";
import { buildLifeRailItems } from "./lifeRailItems";

function surface(id: string, label = id): SurfaceDef {
  return {
    id,
    label,
    icon: "circle",
    kind: "builtin",
    builtinId: id,
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

describe("buildLifeRailItems", () => {
  it("expands Workspace into first-class explorer modes", () => {
    const items = buildLifeRailItems([
      surface("chat", "Chat"),
      surface("work", "Work"),
      surface("library", "Workspace"),
      surface("web", "Web"),
    ]);

    expect(items.map((item) => item.id)).toEqual([
      "chat",
      "work",
      ...LME_EXPLORER_MODES.map((mode) => `lme:${mode.id}`),
      "web",
    ]);
    expect(items.filter((item) => item.kind === "lme-mode")).toHaveLength(8);
  });

  it("keeps library out of the rail as its own destination", () => {
    const items = buildLifeRailItems([surface("library", "Workspace"), surface("chat", "Chat")]);
    expect(items.some((item) => item.kind === "surface" && item.id === "library")).toBe(false);
  });
});
