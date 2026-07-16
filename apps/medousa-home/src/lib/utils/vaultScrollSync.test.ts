import { describe, expect, it } from "vitest";
import {
  applyScrollRatio,
  createVaultScrollSync,
  scrollRatio,
} from "./vaultScrollSync";

function fakeScrollEl(scrollTop: number, scrollHeight: number, clientHeight: number) {
  return {
    scrollTop,
    scrollHeight,
    clientHeight,
  } as HTMLElement;
}

describe("vaultScrollSync", () => {
  it("computes scroll ratio from scrollTop", () => {
    expect(scrollRatio(fakeScrollEl(50, 200, 100))).toBeCloseTo(0.5);
    expect(scrollRatio(fakeScrollEl(0, 100, 100))).toBe(0);
  });

  it("applies ratio to target scrollTop", () => {
    const el = fakeScrollEl(0, 300, 100);
    applyScrollRatio(el, 0.5);
    expect(el.scrollTop).toBeCloseTo(100);
  });

  it("locks against feedback loops", () => {
    const sync = createVaultScrollSync(10_000);
    const a = fakeScrollEl(80, 200, 100); // ratio 0.8
    const b = fakeScrollEl(0, 400, 100); // max scroll 300
    sync.sync(a, b);
    expect(b.scrollTop).toBeCloseTo(240);
    // Second sync while locked should be ignored.
    a.scrollTop = 0;
    sync.sync(a, b);
    expect(b.scrollTop).toBeCloseTo(240);
  });
});
