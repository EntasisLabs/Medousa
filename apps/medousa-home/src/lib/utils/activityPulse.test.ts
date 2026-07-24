import { describe, expect, it } from "vitest";
import {
  isActivityFeedHot,
  truncateActivityLabel,
  ACTIVITY_HOT_MS,
} from "./activityPulse";

describe("isActivityFeedHot", () => {
  it("is hot for recent timestamps", () => {
    const now = Date.parse("2026-07-23T18:00:00.000Z");
    expect(
      isActivityFeedHot(
        new Date(now - ACTIVITY_HOT_MS + 500).toISOString(),
        now,
      ),
    ).toBe(true);
  });

  it("is cold after the window", () => {
    const now = Date.parse("2026-07-23T18:00:00.000Z");
    expect(
      isActivityFeedHot(
        new Date(now - ACTIVITY_HOT_MS - 1).toISOString(),
        now,
      ),
    ).toBe(false);
  });

  it("treats missing/invalid as cold", () => {
    expect(isActivityFeedHot(null)).toBe(false);
    expect(isActivityFeedHot("not-a-date")).toBe(false);
  });
});

describe("truncateActivityLabel", () => {
  it("leaves short labels alone", () => {
    expect(truncateActivityLabel("You updated Daily")).toBe("You updated Daily");
  });

  it("truncates long labels", () => {
    const label = "a".repeat(50);
    expect(truncateActivityLabel(label, 10)).toBe("aaaaaaaaa…");
  });
});
