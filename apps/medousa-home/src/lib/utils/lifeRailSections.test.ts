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
  it("follows preset order and skips the library host surface", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("peers", "Peers"),
      surface("messaging", "Messaging"),
      surface("calendar", "Calendar"),
      surface("work", "Work"),
      surface("web", "Web"),
      surface("library", "Workspace"),
      surface("notes", "Notes"),
      surface("files", "Files"),
      surface("artifacts", "Artifacts"),
      surface("automations", "Automations"),
      surface("map", "Map"),
      surface("runtime", "Runtime"),
      surface("settings", "Settings"),
    ]);

    expect(layout.primary.map((item) => item.id)).toEqual([
      "chat",
      "peers",
      "calendar",
      "work",
      "web",
      "notes",
      "files",
      "artifacts",
      "automations",
      "map",
    ]);
    expect(layout.focusStartIndex).toBe(2);
    expect(layout.showAutomations).toBe(true);
    expect(layout.customStartIndex).toBe(-1);
    expect(layout.you.id).toBe("profiles");
  });

  it("respects a reordered preset sequence", () => {
    const layout = buildLifeRailLayout([
      surface("web", "Web"),
      surface("chat", "Chat"),
      surface("calendar", "Calendar"),
      surface("notes", "Notes"),
    ]);
    expect(layout.primary.map((item) => item.id)).toEqual([
      "web",
      "chat",
      "calendar",
      "notes",
    ]);
    expect(layout.focusStartIndex).toBe(0);
  });

  it("keeps Notes/Files/Artifacts and Automations independent in the primary strip", () => {
    const notesOnly = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("notes", "Notes"),
    ]);
    expect(notesOnly.showAutomations).toBe(false);
    expect(notesOnly.primary.map((item) => item.id)).toEqual(["chat", "notes"]);

    const automationsFirst = buildLifeRailLayout([
      surface("automations", "Automations"),
      surface("chat", "Chat"),
      surface("notes", "Notes"),
      surface("files", "Files"),
      surface("artifacts", "Artifacts"),
    ]);
    expect(automationsFirst.primary.map((item) => item.id)).toEqual([
      "automations",
      "chat",
      "notes",
      "files",
      "artifacts",
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

  it("keeps You as a dock door (not nested, not primary) and Map in the primary strip", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("map", "Map"),
      surface("context", "Context"),
    ]);
    expect(layout.you.kind === "surface" && layout.you.surface.label).toBe("You");
    expect(layout.primary.map((item) => item.id)).toEqual(["chat", "map"]);
    expect(railSectionForItemId("map")).toBe("channels");
    expect(railSectionForItemId("profiles")).toBe("memory");
    expect(railSectionForItemId("library")).toBe("library");
    expect(railSectionForItemId("notes")).toBe("library");
    expect(railSectionForItemId("artifacts")).toBe("library");
  });

  it("never puts runtime, settings, or library host in the rail layout", () => {
    const layout = buildLifeRailLayout([
      surface("runtime", "Runtime"),
      surface("settings", "Settings"),
      surface("library", "Workspace"),
      surface("chat", "Chat"),
    ]);
    const ids = layout.primary.map((item) => item.id);
    expect(ids).not.toContain("runtime");
    expect(ids).not.toContain("settings");
    expect(ids).not.toContain("library");
    expect(layout.showAutomations).toBe(false);
  });
});

describe("buildLifeRailSections (legacy mapping)", () => {
  it("maps Notes/Files/Artifacts and Automations as sibling doors", () => {
    const sections = buildLifeRailSections([
      surface("chat", "Chat"),
      surface("notes", "Notes"),
      surface("files", "Files"),
      surface("artifacts", "Artifacts"),
      surface("automations", "Automations"),
      surface("map", "Map"),
    ]);
    const library = sections.find((section) => section.id === "library");
    expect(library?.items.map((item) => item.id)).toEqual([
      "notes",
      "files",
      "artifacts",
    ]);
    const automations = sections.find((section) => section.id === "automations");
    expect(automations?.items.map((item) => item.id)).toEqual(["automations"]);
  });
});
