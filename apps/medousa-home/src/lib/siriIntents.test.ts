import { describe, expect, it } from "vitest";
import { siriSpokenSummary } from "./siriIntents";

describe("siriSpokenSummary", () => {
  it("turns common markdown into speech-friendly text", () => {
    expect(
      siriSpokenSummary("## Answer\n- Visit [Medousa](https://example.com). **Done.**"),
    ).toBe("Answer Visit Medousa. Done.");
  });

  it("omits fenced code and bounds long answers", () => {
    expect(siriSpokenSummary("Before```swift\nlet secret = 1\n```After")).toBe(
      "Before Code omitted. After",
    );
    const bounded = siriSpokenSummary("word ".repeat(200));
    expect(bounded.length).toBeLessThanOrEqual(600);
    expect(bounded).toMatch(/…$/);
  });
});
