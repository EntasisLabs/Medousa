import { describe, expect, it } from "vitest";
import {
  SETTINGS_SECTIONS,
  settingsNavEntries,
  settingsSectionById,
} from "./settings";

describe("settings nav chapters", () => {
  it("keeps every section in a TOC chapter", () => {
    for (const section of SETTINGS_SECTIONS) {
      expect(section.group).toBeTruthy();
      expect(settingsSectionById(section.id)?.group).toBe(section.group);
    }
  });

  it("emits group headers before their sections", () => {
    const entries = settingsNavEntries();
    expect(entries[0]).toMatchObject({ kind: "group", id: "space" });
    const groups = entries.filter((entry) => entry.kind === "group").map((entry) => entry.id);
    expect(groups).toEqual(["space", "her", "tools", "people", "machine"]);
  });

  it("places engine under tools and connection under machine", () => {
    expect(settingsSectionById("engine")?.group).toBe("tools");
    expect(settingsSectionById("basement")?.group).toBe("machine");
    expect(settingsSectionById("basement")?.label).toBe("Connection");
  });

  it("keeps a single Preferences entry under space", () => {
    expect(settingsSectionById("preferences")?.label).toBe("Preferences");
    expect(settingsSectionById("preferences")?.group).toBe("space");
    expect(SETTINGS_SECTIONS.filter((section) => section.group === "space")).toHaveLength(1);
  });
});
