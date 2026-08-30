import { afterEach, describe, expect, it } from "vitest";

import {
  MOBILE_CALENDAR_SCHEDULE_DAYS,
  calendar,
  calendarDateUtils,
} from "./calendar.svelte";

const initialViewMode = calendar.viewMode;
const initialSelectedDay = calendar.selectedDay;

afterEach(() => {
  calendar.viewMode = initialViewMode;
  calendar.selectedDay = initialSelectedDay;
});

describe("calendar mobile schedule", () => {
  it("loads a forward-looking range without including earlier days", () => {
    calendar.viewMode = "schedule";
    calendar.selectedDay = new Date(2026, 7, 30, 14, 30);

    const { from, to } = calendar.rangeForView();
    const expectedEnd = calendarDateUtils.addDays(
      from,
      MOBILE_CALENDAR_SCHEDULE_DAYS,
    );

    expect(calendarDateUtils.isoDay(from)).toBe("2026-08-30");
    expect(calendarDateUtils.isoDay(to)).toBe(calendarDateUtils.isoDay(expectedEnd));
  });
});
