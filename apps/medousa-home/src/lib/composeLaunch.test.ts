import { get } from "svelte/store";
import { beforeEach, describe, expect, it } from "vitest";
import { pendingComposeLaunch, queueComposeLaunch } from "$lib/composeLaunch";

describe("compose launch queue", () => {
  beforeEach(() => pendingComposeLaunch.set(null));

  it("retains cold-start actions and deduplicates receipts", () => {
    const request = { kind: "compose", action: "camera", requestId: crypto.randomUUID() } as const;
    queueComposeLaunch(request);
    expect(get(pendingComposeLaunch)).toEqual(request);
    pendingComposeLaunch.set(null);
    queueComposeLaunch(request);
    expect(get(pendingComposeLaunch)).toBeNull();
  });
});
