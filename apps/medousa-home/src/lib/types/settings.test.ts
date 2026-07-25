import { describe, expect, it } from "vitest";
import {
  SETTINGS_SECTIONS,
  settingsNavEntries,
  settingsSectionById,
} from "./settings";

describe("settings nav groups", () => {
  it("keeps every section in a TOC group", () => {
    for (const section of SETTINGS_SECTIONS) {
      expect(section.group).toBeTruthy();
      expect(settingsSectionById(section.id)?.group).toBe(section.group);
    }
  });

  it("shows only the This Mac header", () => {
    const entries = settingsNavEntries();
    expect(entries[0]).toMatchObject({ kind: "section", section: { id: "preferences" } });
    const groups = entries.filter((entry) => entry.kind === "group");
    expect(groups.map((entry) => entry.id)).toEqual(["machine"]);
    expect(groups.map((entry) => entry.label)).toEqual(["This Mac"]);
  });

  it("clusters app and this-mac sections with Sharing / Workshop labels", () => {
    expect(settingsSectionById("preferences")?.group).toBe("app");
    expect(settingsSectionById("agent")?.group).toBe("app");
    expect(settingsSectionById("runtime")?.group).toBe("app");
    expect(settingsSectionById("network")?.label).toBe("Sharing");
    expect(settingsSectionById("network")?.hint).toBe("Seats, phone, peers & channels");
    expect(settingsSectionById("network")?.group).toBe("app");
    expect(SETTINGS_SECTIONS.filter((section) => section.group === "app")).toHaveLength(4);

    expect(settingsSectionById("packages")?.group).toBe("machine");
    expect(settingsSectionById("mcp")?.group).toBe("machine");
    expect(settingsSectionById("mcp")?.label).toBe("MCP");
    expect(settingsSectionById("basement")?.group).toBe("machine");
    expect(settingsSectionById("basement")?.label).toBe("Workshop");
    expect(settingsSectionById("basement")?.hint).toBe("Active workshop, engine & files");
    expect(SETTINGS_SECTIONS.filter((section) => section.group === "machine")).toHaveLength(3);
  });
});
