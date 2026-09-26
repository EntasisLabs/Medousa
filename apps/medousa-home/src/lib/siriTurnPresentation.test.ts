import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearSiriOwnedTurnsForTests,
  registerSiriOwnedTurn,
  shouldSuppressSiriTurnNotification,
} from "./siriTurnPresentation";

describe("Siri turn presentation", () => {
  beforeEach(() => {
    clearSiriOwnedTurnsForTests();
    vi.restoreAllMocks();
  });

  it("suppresses one notification while Siri owns the result", () => {
    vi.spyOn(Date, "now").mockReturnValue(1_000);
    registerSiriOwnedTurn("turn-1", 18_000);
    expect(shouldSuppressSiriTurnNotification("turn-1", 19_000)).toBe(true);
    expect(shouldSuppressSiriTurnNotification("turn-1", 19_000)).toBe(false);
  });

  it("allows a notification after the Siri result window", () => {
    vi.spyOn(Date, "now").mockReturnValue(1_000);
    registerSiriOwnedTurn("turn-1", 18_000);
    expect(shouldSuppressSiriTurnNotification("turn-1", 19_001)).toBe(false);
  });
});
