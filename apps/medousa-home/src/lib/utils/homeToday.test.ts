import { describe, expect, it } from "vitest";
import type { CalendarEvent } from "$lib/types/calendar";
import { homeTodayAgenda } from "$lib/utils/homeToday";

function event(
  uid: string,
  summary: string,
  dtstart: string,
  options: Partial<CalendarEvent> = {},
): CalendarEvent {
  return {
    uid,
    summary,
    dtstart,
    dtend: null,
    all_day: false,
    calendar_path: "calendar/personal.ics",
    ...options,
  };
}

describe("homeTodayAgenda", () => {
  const now = new Date("2026-08-29T14:30:00");

  it("omits finished events and keeps ongoing and upcoming events", () => {
    const agenda = homeTodayAgenda(
      [
        event("past", "Breakfast", "2026-08-29T08:00:00", {
          dtend: "2026-08-29T09:00:00",
        }),
        event("now", "Mom's appointment", "2026-08-29T14:00:00", {
          dtend: "2026-08-29T15:00:00",
        }),
        event("next", "Dinner", "2026-08-29T18:00:00"),
      ],
      now,
    );

    expect(agenda.rows.map((row) => row.event.uid)).toEqual(["now", "next"]);
    expect(agenda.rows[0].timeLabel).toBe("Now");
    expect(agenda.rows[0].timing).toBe("now");
  });

  it("puts all-day events first and gives them a human time label", () => {
    const agenda = homeTodayAgenda(
      [
        event("later", "Call", "2026-08-29T16:00:00"),
        event("all-day", "Birthday", "2026-08-29", { all_day: true }),
      ],
      now,
    );

    expect(agenda.rows.map((row) => row.event.uid)).toEqual(["all-day", "later"]);
    expect(agenda.rows[0].timeLabel).toBe("All day");
  });

  it("caps Home to three rows and reports the rest", () => {
    const agenda = homeTodayAgenda(
      [
        event("a", "One", "2026-08-29T15:00:00"),
        event("b", "Two", "2026-08-29T16:00:00"),
        event("c", "Three", "2026-08-29T17:00:00"),
        event("d", "Four", "2026-08-29T18:00:00"),
      ],
      now,
      3,
    );

    expect(agenda.rows).toHaveLength(3);
    expect(agenda.hiddenCount).toBe(1);
  });

  it("stays silent when nothing remains today", () => {
    const agenda = homeTodayAgenda(
      [
        event("past", "Earlier", "2026-08-29T09:00:00", {
          dtend: "2026-08-29T10:00:00",
        }),
      ],
      now,
    );

    expect(agenda).toEqual({ rows: [], hiddenCount: 0 });
  });

  it("does not leak tomorrow's events into Today", () => {
    const agenda = homeTodayAgenda(
      [event("tomorrow", "Tomorrow", "2026-08-30T09:00:00")],
      now,
    );

    expect(agenda.rows).toEqual([]);
  });
});
