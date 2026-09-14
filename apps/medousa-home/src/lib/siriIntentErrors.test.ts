import { describe, expect, it } from "vitest";
import { classifySiriAskFailure } from "./siriIntentErrors";

describe("classifySiriAskFailure", () => {
  it.each([
    ["Siri request expired or was already consumed", "expired"],
    ["another active turn is already working", "busy"],
    ["selected workshop has no authenticated session", "authentication"],
    ["connection refused", "offline"],
    ["No active workshop in registry", "workshop_unavailable"],
    ["something surprising happened", "unknown"],
  ] as const)("maps %s to %s", (message, code) => {
    expect(classifySiriAskFailure(new Error(message)).code).toBe(code);
  });
});
