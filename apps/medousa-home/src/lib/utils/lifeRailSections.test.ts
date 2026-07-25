import { describe, expect, it } from "vitest";
import type { SurfaceDef } from "$lib/types/environment";
import {
  buildLifeRailLayout,
  buildLifeRailSections,
  railSectionForItemId,
} from "./lifeRailSections";

function surface(
  id: string,
  label = id,
  kind: SurfaceDef["kind"] = "builtin",
): SurfaceDef {
  return {
    id,
    label,
    icon: "circle",
    kind,
    builtinId: id,
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

describe("buildLifeRailLayout", () => {
  it("follows preset order and keeps Library / Automations in the primary strip", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("peers", "Peers"),
      surface("messaging", "Messaging"),
      surface("calendar", "Calendar"),
      surface("work", "Work"),
      surface("web", "Web"),
      surface("library", "Workspace"),
      surface("automations", "Automations"),
      surface("context", "Context"),
      surface("runtime", "Runtime"),
      surface("settings", "Settings"),
    ]);

    expect(layout.primary.map((item) => item.id)).toEqual([
      "chat",
      "peers",
      "calendar",
      "work",
      "web",
      "library",
      "automations",
    ]);
    expect(layout.focusStartIndex).toBe(2);
    expect(layout.showLibrary).toBe(true);
    expect(layout.showAutomations).toBe(true);
    expect(layout.customStartIndex).toBe(-1);
    expect(layout.you.id).toBe("profiles");
    expect(layout.context?.id).toBe("context");
  });

  it("respects a reordered preset sequence", () => {
    const layout = buildLifeRailLayout([
      surface("web", "Web"),
      surface("chat", "Chat"),
      surface("calendar", "Calendar"),
      surface("library", "Library"),
    ]);
    expect(layout.primary.map((item) => item.id)).toEqual([
      "web",
      "chat",
      "calendar",
      "library",
    ]);
    expect(layout.focusStartIndex).toBe(0);
  });

  it("keeps Library and Automations independent in the primary strip", () => {
    const libraryOnly = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("library", "Workspace"),
    ]);
    expect(libraryOnly.showLibrary).toBe(true);
    expect(libraryOnly.showAutomations).toBe(false);
    expect(libraryOnly.primary.map((item) => item.id)).toEqual(["chat", "library"]);

    const automationsFirst = buildLifeRailLayout([
      surface("automations", "Automations"),
      surface("chat", "Chat"),
      surface("library", "Workspace"),
    ]);
    expect(automationsFirst.primary.map((item) => item.id)).toEqual([
      "automations",
      "chat",
      "library",
    ]);
    expect(automationsFirst.showAutomations).toBe(true);
  });

  it("promotes custom surfaces as primary peers in place", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("bug-tracker", "Bug Tracker", "custom"),
      surface("web", "Web"),
    ]);
    expect(layout.primary.map((item) => item.id)).toEqual([
      "chat",
      "bug-tracker",
      "web",
    ]);
    expect(layout.customStartIndex).toBe(1);
  });

  it("keeps Context as a dock sibling next to You (not nested, not primary)", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("context", "Context"),
    ]);
    expect(layout.you.kind === "surface" && layout.you.surface.label).toBe("You");
    expect(layout.primary.map((item) => item.id)).toEqual(["chat"]);
    expect(layout.context?.id).toBe("context");
    expect(railSectionForItemId("context")).toBe("memory");
    expect(railSectionForItemId("profiles")).toBe("memory");
    expect(railSectionForItemId("library")).toBe("library");
  });

  it("never puts runtime or settings in the rail layout", () => {
    const layout = buildLifeRailLayout([
      surface("runtime", "Runtime"),
      surface("settings", "Settings"),
      surface("chat", "Chat"),
    ]);
    const ids = layout.primary.map((item) => item.id);
    expect(ids).not.toContain("runtime");
    expect(ids).not.toContain("settings");
    expect(layout.showLibrary).toBe(false);
    expect(layout.showAutomations).toBe(false);
  });
});

describe("buildLifeRailSections (legacy mapping)", () => {
  it("maps Library and Automations as sibling doors", () => {
    const sections = buildLifeRailSections([
      surface("chat", "Chat"),
      surface("library", "Workspace"),
      surface("automations", "Automations"),
      surface("context", "Context"),
    ]);
    const library = sections.find((section) => section.id === "library");
    expect(library?.items.map((item) => item.id)).toEqual(["library", "automations"]);
  });
});
