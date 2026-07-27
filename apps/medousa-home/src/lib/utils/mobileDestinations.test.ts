import { describe, expect, it } from "vitest";
import { mobileDestinationSections } from "$lib/utils/mobileDestinations";

describe("mobileDestinationSections", () => {
  it("lists primary tabs then more destinations", () => {
    const sections = mobileDestinationSections();
    expect(sections[0]?.title).toBe("Go to");
    expect(sections[0]?.items.some((item) => item.id === "tab-home")).toBe(true);
    expect(sections[0]?.items.some((item) => item.id === "tab-chat")).toBe(true);
    expect(sections[1]?.title).toBe("More");
    expect(sections[1]?.items.some((item) => item.more === "automations")).toBe(true);
    expect(sections[1]?.items.some((item) => item.more === "settings")).toBe(true);
  });
});
