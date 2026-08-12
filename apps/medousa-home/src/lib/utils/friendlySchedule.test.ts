import { describe, expect, it } from "vitest";
import {
  cronToFriendly,
  friendlyToCron,
  toSevenFieldCron,
} from "$lib/utils/friendlySchedule";

describe("toSevenFieldCron", () => {
  it("expands 5-field unix cron", () => {
    expect(toSevenFieldCron("0 9 * * *")).toBe("0 0 9 * * * *");
  });

  it("expands 6-field cron", () => {
    expect(toSevenFieldCron("0 0 */4 * * *")).toBe("0 0 */4 * * * *");
  });

  it("leaves 7-field cron alone", () => {
    expect(toSevenFieldCron("0 0 9 * * 1-5 *")).toBe("0 0 9 * * 1-5 *");
  });
});

describe("friendlyToCron", () => {
  it("emits 7-field daily schedules", () => {
    expect(
      friendlyToCron({
        frequency: "daily",
        hour: 9,
        minute: 0,
        weekday: 1,
        customCron: "",
      }),
    ).toBe("0 0 9 * * * *");
  });
});

describe("cronToFriendly", () => {
  it("parses 7-field cron as daily", () => {
    const state = cronToFriendly("0 30 17 * * * *");
    expect(state.frequency).toBe("daily");
    expect(state.hour).toBe(17);
    expect(state.minute).toBe(30);
  });

  it("still parses legacy 5-field cron", () => {
    const state = cronToFriendly("15 8 * * 1-5");
    expect(state.frequency).toBe("weekdays");
    expect(state.hour).toBe(8);
    expect(state.minute).toBe(15);
  });
});
