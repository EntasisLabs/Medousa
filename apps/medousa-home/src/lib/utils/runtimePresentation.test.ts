import { describe, expect, it } from "vitest";
import { formatWorkshopUpdatedAt } from "./runtimePresentation";

describe("formatWorkshopUpdatedAt", () => {
  const now = new Date("2026-08-30T12:00:00.000Z").getTime();

  it("uses quiet relative timestamps", () => {
    expect(formatWorkshopUpdatedAt("2026-08-30T11:59:40.000Z", now)).toBe("just now");
    expect(formatWorkshopUpdatedAt("2026-08-30T11:42:00.000Z", now)).toBe("18m ago");
    expect(formatWorkshopUpdatedAt("2026-08-30T09:00:00.000Z", now)).toBe("3h ago");
  });

  it("returns nothing for missing or invalid timestamps", () => {
    expect(formatWorkshopUpdatedAt(null, now)).toBe("");
    expect(formatWorkshopUpdatedAt("not-a-date", now)).toBe("");
  });
});
