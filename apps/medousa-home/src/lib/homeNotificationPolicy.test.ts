import { describe, expect, it } from "vitest";
import { shouldPresentHomeNotification } from "./homeNotificationPolicy";

describe("home notification policy", () => {
  it("always presents fatal failures and scheduled deliveries", () => {
    const disabled = { turnUpdates: false, needsInput: false };
    expect(shouldPresentHomeNotification("fatal_turn", disabled)).toBe(true);
    expect(shouldPresentHomeNotification("scheduled_delivery", disabled)).toBe(true);
  });

  it("respects independent turn-update and needs-input preferences", () => {
    expect(
      shouldPresentHomeNotification("turn_update", {
        turnUpdates: false,
        needsInput: true,
      }),
    ).toBe(false);
    expect(
      shouldPresentHomeNotification("needs_input", {
        turnUpdates: false,
        needsInput: true,
      }),
    ).toBe(true);
  });
});
