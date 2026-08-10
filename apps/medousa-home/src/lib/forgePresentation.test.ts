import { describe, expect, it } from "vitest";
import {
  humanExecutorLabel,
  humanPhaseGuidance,
  humanPhaseLabel,
  humanizeForgeMessage,
} from "$lib/forge";

describe("Forge presentation language", () => {
  it("turns internal phases into user orientation", () => {
    expect(humanPhaseLabel("prepare")).toBe("Ready to set up");
    expect(humanPhaseLabel("review")).toBe("Ready to review");
    expect(humanPhaseLabel("complete")).toBe("Finished");
    expect(humanPhaseLabel("future_internal_state")).toBe("In progress");
    expect(humanPhaseGuidance("needs_attention")).toContain("decision");
  });

  it("describes collaborators without leaking executor kinds", () => {
    expect(humanExecutorLabel("human")).toBe("You");
    expect(humanExecutorLabel("codex")).toBe("Codex");
    expect(humanExecutorLabel("unknown-runtime")).toBe("Agent");
  });

  it("keeps infrastructure vocabulary out of errors", () => {
    expect(
      humanizeForgeMessage(
        "The undertaking needs an active lease before source file changes in its governed workspace",
      ),
    ).toBe(
      "The project needs an active editing session before file changes in its project",
    );
    expect(humanizeForgeMessage("The working copy will be released.")).toBe(
      "The working copy will be released.",
    );
  });

  it("explains empty workshop 404s as a stale daemon", () => {
    expect(humanizeForgeMessage("workshop returned HTTP 404 Not Found:")).toContain(
      "Rebuild and restart medousa_daemon",
    );
    expect(humanizeForgeMessage("workshop returned HTTP 404 Not Found: ")).toContain(
      "older than Medousa",
    );
  });

  it("explains missing indexed snapshots as a code map that is not ready", () => {
    expect(
      humanizeForgeMessage(
        "workshop returned HTTP 404 Not Found: work work-18c6f16a996b12f8988cb3bd84be88f8 has no ready indexed snapshot yet",
      ),
    ).toBe("The code map isn’t ready yet. Rebuild it, or wait for indexing to finish.");
    expect(
      humanizeForgeMessage(
        "HTTP 404 Not Found for /v1/world/works/work-abc/code_avec",
      ),
    ).toContain("code map isn’t ready");
  });
});
