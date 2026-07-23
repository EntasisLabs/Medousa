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
  it("keeps a flat primary strip with Library and Automations as doors", () => {
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
      "messaging",
      "calendar",
      "work",
      "web",
    ]);
    expect(layout.focusStartIndex).toBe(3);
    expect(layout.showLibrary).toBe(true);
    expect(layout.showAutomations).toBe(true);
    expect(layout.customStartIndex).toBe(-1);
    expect(layout.you.id).toBe("profiles");
    expect(layout.context?.id).toBe("context");
  });

  it("shows Automations whenever Library is present, even if Automations was dropped from the preset", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("library", "Workspace"),
    ]);
    expect(layout.showLibrary).toBe(true);
    expect(layout.showAutomations).toBe(true);
  });

  it("promotes custom surfaces as primary peers", () => {
    const layout = buildLifeRailLayout([
      surface("chat", "Chat"),
      surface("web", "Web"),
      surface("bug-tracker", "Bug Tracker", "custom"),
    ]);
    expect(layout.primary.map((item) => item.id)).toEqual([
      "chat",
      "web",
      "bug-tracker",
    ]);
    expect(layout.customStartIndex).toBe(2);
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
