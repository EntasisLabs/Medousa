import { describe, expect, it } from "vitest";
import { thisHostLabel } from "$lib/platformCopy";
import {
  SETTINGS_MOBILE_SECTIONS,
  SETTINGS_SECTIONS,
  settingsMobileSections,
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

  it("labels Medousa and the current host without hardcoding an OS", () => {
    const entries = settingsNavEntries();
    expect(entries[0]).toMatchObject({ kind: "group", id: "app", label: "Medousa" });
    const groups = entries.filter((entry) => entry.kind === "group");
    expect(groups.map((entry) => entry.id)).toEqual(["app", "machine"]);
    expect(groups.map((entry) => entry.label)).toEqual(["Medousa", thisHostLabel()]);
  });

  it("clusters app and this-host sections with Sharing / Connection labels", () => {
    expect(settingsSectionById("preferences")?.group).toBe("app");
    expect(settingsSectionById("agent")?.group).toBe("app");
    expect(settingsSectionById("runtime")?.group).toBe("app");
    expect(settingsSectionById("network")?.label).toBe("Sharing");
    expect(settingsSectionById("network")?.hint).toBe("Seats, phone, peers & channels");
    expect(settingsSectionById("network")?.group).toBe("app");
    expect(settingsSectionById("connections")?.group).toBe("app");
    expect(settingsSectionById("connections")?.label).toBe("Connections");
    expect(SETTINGS_SECTIONS.filter((section) => section.group === "app")).toHaveLength(5);

    expect(settingsSectionById("packages")?.group).toBe("machine");
    expect(settingsSectionById("mcp")?.group).toBe("machine");
    expect(settingsSectionById("mcp")?.label).toBe("MCP");
    expect(settingsSectionById("basement")?.group).toBe("machine");
    expect(settingsSectionById("basement")?.label).toBe("Connection");
    expect(settingsSectionById("basement")?.hint).toBe("Active workshop, engine & files");
    expect(SETTINGS_SECTIONS.filter((section) => section.group === "machine")).toHaveLength(3);
  });

  it("rotates the mobile pager through every settings section", () => {
    expect(SETTINGS_MOBILE_SECTIONS).toEqual(SETTINGS_SECTIONS.map((section) => section.id));
  });

  it("keeps host-only sections in the pager outside the companion shell", () => {
    expect(settingsMobileSections()).toEqual(SETTINGS_MOBILE_SECTIONS);
  });
});
