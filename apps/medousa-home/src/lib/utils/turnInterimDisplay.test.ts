import { describe, expect, it } from "vitest";
import { stageWhisperAfterFinish } from "./turnInterimDisplay";

describe("stageWhisperAfterFinish", () => {
  it("promotes statusLine that differs from final content", () => {
    expect(
      stageWhisperAfterFinish("Let me check…", "Here is the answer.", null),
    ).toBe("Let me check…");
  });

  it("does not promote statusLine identical to final content", () => {
    expect(stageWhisperAfterFinish("Same text", "Same text", null)).toBeNull();
  });

  it("keeps existing stageWhisper when statusLine is empty", () => {
    expect(stageWhisperAfterFinish(null, "Final answer", "Prior whisper")).toBe(
      "Prior whisper",
    );
  });

  it("prefers statusLine over existing stageWhisper when distinct", () => {
    expect(
      stageWhisperAfterFinish("New interim", "Final answer", "Old whisper"),
    ).toBe("New interim");
  });
});
