import { describe, expect, it } from "vitest";
import {
  parseCalloutRaw,
  serializeCalloutRaw,
} from "./liveCalloutSurface";

describe("liveCalloutSurface", () => {
  it("round-trips tone, title, and body", () => {
    const raw = [
      "```callout",
      "tone: warn",
      "title: Heads up",
      "body: Check the ledger.",
      "```",
      "",
    ].join("\n");
    const model = parseCalloutRaw(raw);
    expect(model).toEqual({
      tone: "warn",
      title: "Heads up",
      body: "Check the ledger.",
    });
    expect(serializeCalloutRaw(model)).toContain("tone: warn");
    expect(serializeCalloutRaw(model)).toContain("title: Heads up");
    expect(serializeCalloutRaw(model)).toContain("body: Check the ledger.");
  });
});
