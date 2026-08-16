import { describe, expect, it, vi } from "vitest";
import { probeClientPlatform } from "./platformProbe";

describe("probeClientPlatform", () => {
  it("selects mobile on a narrow viewport", () => {
    vi.stubGlobal("window", {
      matchMedia: (query: string) => ({
        matches: query.includes("max-width"),
        addEventListener() {},
        removeEventListener() {},
      }),
    });
    expect(probeClientPlatform()).toBe("mobile");
    vi.unstubAllGlobals();
  });
});
