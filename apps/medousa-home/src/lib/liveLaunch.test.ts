import { get } from "svelte/store";
import { describe, expect, it } from "vitest";
import { pendingLiveLaunch, queueLiveLaunch } from "./liveLaunch";

describe("Live launch routing", () => {
  it("retains a launch before mounting and ignores replay after consumption", () => {
    const request = { kind: "live" as const, mode: "new" as const, requestId: "first" };
    queueLiveLaunch(request);
    expect(get(pendingLiveLaunch)).toEqual(request);
    pendingLiveLaunch.set(null);
    queueLiveLaunch(request);
    expect(get(pendingLiveLaunch)).toBeNull();
  });

  it("bounds pending work to the latest launch", () => {
    queueLiveLaunch({ kind: "live", mode: "new", requestId: "second" });
    queueLiveLaunch({ kind: "live", mode: "resume", requestId: "third" });
    expect(get(pendingLiveLaunch)?.mode).toBe("resume");
    pendingLiveLaunch.set(null);
  });
});
