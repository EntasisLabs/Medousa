import { describe, expect, it } from "vitest";
import { compatibleReasoning, normalizeReasoningCapability, normalizeReasoningEffort, UNKNOWN_REASONING } from "./reasoningEffort";

describe("model reasoning compatibility", () => {
  it("distinguishes omission from explicit Off and never exposes unadvertised levels", () => {
    const sol = normalizeReasoningCapability({ kind: "effort", levels: ["none", "low", "high", "future"] });
    expect(sol.levels).toEqual(["none", "low", "high"]);
    expect(normalizeReasoningEffort("none")).toBe("none");
    expect(compatibleReasoning("none", sol)).toBe("none");
    expect(compatibleReasoning("max", sol)).toBe("default");
    expect(compatibleReasoning("high", UNKNOWN_REASONING)).toBe("default");
  });
  it("validates budgets without treating 0 as the provider default", () => {
    const flash = normalizeReasoningCapability({ kind: "budget", budgetMin: 0, budgetMax: 24576 });
    expect(compatibleReasoning("budget:0", flash)).toBe("budget:0");
    expect(compatibleReasoning("budget:24576", flash)).toBe("budget:24576");
    for (const value of ["budget:24577", "budget:-1", "budget:1.5", "max", "none"]) {
      expect(compatibleReasoning(value, flash)).toBe("default");
    }
    expect(normalizeReasoningCapability({ kind: "budget", budgetMin: 10, budgetMax: 5 })).toEqual(UNKNOWN_REASONING);
  });
});
