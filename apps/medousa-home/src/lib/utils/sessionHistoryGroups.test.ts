import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { SessionSummary } from "$lib/types/session";
import { groupSessionsByRecency } from "./sessionHistoryGroups";

function session(id: string, timestamp?: string | null): SessionSummary {
  return {
    session_id: id,
    display_name: id,
    turns: 1,
    verification_runs: 0,
    last_timestamp: timestamp,
    preview: id,
  };
}

describe("groupSessionsByRecency", () => {
  const originalTz = process.env.TZ;

  beforeAll(() => {
    process.env.TZ = "America/Los_Angeles";
  });

  afterAll(() => {
    if (originalTz === undefined) {
      delete process.env.TZ;
    } else {
      process.env.TZ = originalTz;
    }
  });

  it("groups history into useful local-calendar ranges", () => {
    const groups = groupSessionsByRecency(
      [
        session("today", "2026-08-20T08:00:00-07:00"),
        session("yesterday", "2026-08-19T22:00:00-07:00"),
        session("week", "2026-08-14T12:00:00-07:00"),
        session("older", "2026-08-01T12:00:00-07:00"),
        session("missing", null),
      ],
      new Date("2026-08-20T18:00:00-07:00"),
    );

    expect(groups.map((group) => group.label)).toEqual([
      "Today",
      "Yesterday",
      "Previous 7 days",
      "Older",
    ]);
    expect(groups.map((group) => group.sessions.map((item) => item.session_id))).toEqual([
      ["today"],
      ["yesterday"],
      ["week"],
      ["older", "missing"],
    ]);
  });

  it("omits empty ranges and preserves session order", () => {
    const groups = groupSessionsByRecency(
      [
        session("newer", "2026-08-20T17:00:00-07:00"),
        session("older", "2026-08-20T09:00:00-07:00"),
      ],
      new Date("2026-08-20T18:00:00-07:00"),
    );

    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({ id: "today", label: "Today" });
    expect(groups[0].sessions.map((item) => item.session_id)).toEqual(["newer", "older"]);
  });
});
