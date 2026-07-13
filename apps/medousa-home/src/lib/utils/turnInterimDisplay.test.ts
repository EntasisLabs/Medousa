import { describe, expect, it } from "vitest";
import {
  stageWhisperAfterFinish,
  statusLineAfterScratchReset,
} from "./turnInterimDisplay";

describe("statusLineAfterScratchReset", () => {
  it("promotes draft content into the status line", () => {
    expect(statusLineAfterScratchReset("Let me check that…", null)).toBe(
      "Let me check that…",
    );
  });

  it("keeps prior status when draft is empty", () => {
    expect(statusLineAfterScratchReset("", "Searching…")).toBe("Searching…");
    expect(statusLineAfterScratchReset("   ", "Searching…")).toBe("Searching…");
  });

  it("prefers draft over prior status", () => {
    expect(statusLineAfterScratchReset("New draft", "Old status")).toBe("New draft");
  });
});

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
