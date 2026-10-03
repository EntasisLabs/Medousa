import { describe, expect, it } from "vitest";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import { peerProgressPresentation } from "./peerProgress";

const now = Date.parse("2026-10-03T12:00:00Z");
const row = (progress: unknown) => ({ binding: {}, progress } as PeerProposalReviewRecord);

describe("assignment progress presentation", () => {
  it("keeps a failed action separate from the assignment outcome", () => {
    const result = peerProgressPresentation(row({
      state: "running", observed_at: new Date(now).toISOString(),
      last_activity_at: new Date(now).toISOString(), current_activity: "Repair build",
      last_activity: "Check package", last_activity_status: "failed",
    }), now);
    expect(result.headline).toBe("Agent is working");
    expect(result.activity).toBe("Repair build");
    expect(result.lastActivityStatus).toBe("failed");
    expect(result.notice).toBeNull();
  });

  it.each(["completed", "failed", "cancelled", "interrupted"] as const)("gives a verified %s receipt priority over stale progress", outcome => {
    const record = row({ state: "running", current_activity: "Old action" });
    record.receipt = { outcome } as NonNullable<PeerProposalReviewRecord["receipt"]>;
    const result = peerProgressPresentation(record, now);
    expect(result.headline).toBe(`Work ${outcome}`);
    expect(result.activity).toBeNull();
    expect(result.notice).toBeNull();
  });

  it("reports delayed polling separately from missing execution liveness", () => {
    const progress = { state: "running", observed_at: new Date(now - 60_000).toISOString(), last_activity_at: new Date(now - 60_000).toISOString() };
    expect(peerProgressPresentation(row(progress), now).notice).toContain("updates are delayed");
    const missing = peerProgressPresentation(row({ ...progress, state: "unobserved", observed_at: new Date(now).toISOString() }), now);
    expect(missing.headline).toBe("Execution status unavailable");
    expect(missing.notice).toContain("cannot currently observe");
  });

  it("describes a long-running quiet step without declaring failure", () => {
    const result = peerProgressPresentation(row({ state: "running", observed_at: new Date(now).toISOString(), last_activity_at: new Date(now - 600_000).toISOString() }), now);
    expect(result.lastUpdate).toBe("10m ago");
    expect(result.notice).toContain("No recent activity");
    expect(result.headline).toBe("Agent is working");
  });

  it("remains compatible with workshops that do not report progress", () => {
    expect(peerProgressPresentation(row(undefined), now).notice).toContain("has not reported live progress");
  });
});
