import { describe, expect, it } from "vitest";
import { capabilityBadges } from "$lib/utils/lanShareApi";

describe("lanShareApi", () => {
  it("parses capability bitfield badges", () => {
    expect(capabilityBadges("003F")).toEqual(["Share", "Layouts", "Relay"]);
    expect(capabilityBadges(null)).toEqual([]);
  });
});
