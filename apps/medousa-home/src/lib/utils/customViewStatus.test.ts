import { describe, expect, it } from "vitest";
import type { ComponentDef } from "$lib/types/environment";
import { feedBadgeForComponents, presetDisplayLabel } from "$lib/utils/customViewStatus";

function component(id: string, feeds: string[] = []): ComponentDef {
  return {
    id,
    type: "presentation",
    surfaceId: "trip-london",
    slot: "main",
    config: {},
    feeds,
  };
}

describe("feedBadgeForComponents", () => {
  it("returns none when no feeds subscribed", () => {
    const map = new Map<string, Record<string, unknown>>();
    expect(feedBadgeForComponents([component("a")], map)).toBe("none");
  });

  it("returns live when checkedAt is within 10 minutes", () => {
    const map = new Map([
      ["a", { checkedAt: new Date(Date.now() - 60_000).toISOString() }],
    ]);
    expect(feedBadgeForComponents([component("a", ["trip.london.trains"])], map)).toBe(
      "live",
    );
  });

  it("returns stale when feeds exist but patch is old", () => {
    const map = new Map([
      ["a", { checkedAt: new Date(Date.now() - 20 * 60_000).toISOString() }],
    ]);
    expect(feedBadgeForComponents([component("a", ["trip.london.trains"])], map)).toBe(
      "stale",
    );
  });
});

describe("presetDisplayLabel", () => {
  it("maps built-in preset ids to user-facing labels", () => {
    expect(presetDisplayLabel("default", "Default")).toBe("Full");
    expect(presetDisplayLabel("focus", "Focus")).toBe("Focus");
    expect(presetDisplayLabel("writing", "Writing")).toBe("Writing");
  });
});
