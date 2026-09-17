import { describe, expect, it } from "vitest";
import { shouldNotifyTurnCompletion } from "./turnNotificationPolicy";

describe("mobile turn notifications", () => {
  it("suppresses successful results already being viewed", () => {
    expect(shouldNotifyTurnCompletion(false, false, true)).toBe(false);
    expect(shouldNotifyTurnCompletion(false, false, false)).toBe(true);
    expect(shouldNotifyTurnCompletion(false, true, false)).toBe(false);
  });
  it("keeps failures actionable", () => {
    expect(shouldNotifyTurnCompletion(true, false, true)).toBe(true);
    expect(shouldNotifyTurnCompletion(true, true, false)).toBe(true);
  });
});
