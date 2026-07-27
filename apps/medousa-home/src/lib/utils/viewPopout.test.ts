/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  VIEW_POPOUT_LAST_KEY,
  VIEW_POPOUT_SURFACE_KEY,
  clearViewPopoutSurface,
  readLastViewPopoutSurface,
  readViewPopoutSurface,
  writeViewPopoutSurface,
} from "./viewPopout";

describe("viewPopout handoff", () => {
  afterEach(() => {
    localStorage.removeItem(VIEW_POPOUT_SURFACE_KEY);
    localStorage.removeItem(VIEW_POPOUT_LAST_KEY);
  });

  it("writes surface and last keys", () => {
    writeViewPopoutSurface("custom:arcade");
    expect(readViewPopoutSurface()).toBe("custom:arcade");
    expect(readLastViewPopoutSurface()).toBe("custom:arcade");
  });

  it("clears only the active handoff key", () => {
    writeViewPopoutSurface("custom:desk");
    clearViewPopoutSurface();
    expect(readViewPopoutSurface()).toBeNull();
    expect(readLastViewPopoutSurface()).toBe("custom:desk");
  });
});
