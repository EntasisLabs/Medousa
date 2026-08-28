import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import {
  mobileEditableDestinationItems,
  mobileDestinationSections,
  moreDestinationItems,
  settingsDestinationItem,
} from "$lib/utils/mobileDestinations";

describe("mobileDestinationSections", () => {
  it("lists primary tabs then more destinations without settings", () => {
    const sections = mobileDestinationSections();
    expect(sections[0]?.title).toBe("Go to");
    expect(sections[0]?.items.some((item) => item.id === "tab-home")).toBe(true);
    expect(sections[0]?.items.some((item) => item.id === "tab-chat")).toBe(true);
    expect(sections[0]?.items.some((item) => item.more === "code")).toBe(true);
    expect(sections[1]?.title).toBe("More");
    expect(sections[1]?.items.some((item) => item.more === "automations")).toBe(true);
    expect(moreDestinationItems().some((item) => item.more === "settings")).toBe(false);
  });

  it("keeps settings as a dedicated trailing destination", () => {
    expect(settingsDestinationItem().more).toBe("settings");
  });

  it("projects active layout membership while keeping utility doors", () => {
    const spec = defaultEnvironmentSpec();
    const preset = spec.layoutPresets![0]!;
    preset.surfaces = preset.surfaces.filter(
      (id) => id !== "web" && id !== "workshop" && id !== "automations",
    );

    const items = mobileDestinationSections(spec).flatMap((section) => section.items);
    expect(items.some((item) => item.id === "tab-web")).toBe(false);
    expect(items.some((item) => item.more === "automations")).toBe(false);
    expect(items.some((item) => item.id === "tab-home")).toBe(true);
    expect(items.some((item) => item.more === "profiles")).toBe(true);
    expect(items.some((item) => item.more === "workshop")).toBe(true);
    expect(items.some((item) => item.more === "runtime")).toBe(true);
  });

  it("orders mobile doors using their shared active layout order", () => {
    const spec = defaultEnvironmentSpec();
    spec.layoutPresets![0]!.surfaces = [
      "web",
      "notes",
      "chat",
      "home",
      "settings",
      "runtime",
    ];

    expect(
      mobileDestinationSections(spec)[0]?.items.map((item) => item.id),
    ).toEqual(["tab-home", "tab-web", "tab-notes", "tab-chat"]);
  });

  it("exposes hidden, supported doors to the menu editor", () => {
    const spec = defaultEnvironmentSpec();
    spec.layoutPresets![0]!.surfaces = spec.layoutPresets![0]!.surfaces.filter(
      (id) => id !== "web",
    );

    expect(
      mobileEditableDestinationItems(spec).some((item) => item.surfaceId === "web"),
    ).toBe(true);
    expect(
      mobileEditableDestinationItems(spec).some((item) => item.surfaceId === "home"),
    ).toBe(false);
    expect(
      mobileEditableDestinationItems(spec).some(
        (item) => item.surfaceId === "workshop",
      ),
    ).toBe(false);
  });
});
