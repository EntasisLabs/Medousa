import { describe, expect, it } from "vitest";

import { p02HarnessAvailable } from "./p02Availability";

describe("p02HarnessAvailable", () => {
  it("keeps ordinary production builds sealed", () => {
    expect(p02HarnessAvailable(false, undefined)).toBe(false);
    expect(p02HarnessAvailable(false, "0")).toBe(false);
  });

  it("allows development and explicit benchmark builds", () => {
    expect(p02HarnessAvailable(true, undefined)).toBe(true);
    expect(p02HarnessAvailable(false, "1")).toBe(true);
  });
});
