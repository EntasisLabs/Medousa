import { describe, expect, it, vi, afterEach } from "vitest";
import {
  placeComposerSlashMenuAnchor,
  placeSlashMenuAnchor,
} from "./slashMenuPlacement";

function fakeShell(box: {
  top: number;
  left: number;
  width: number;
  height: number;
}): HTMLElement {
  return {
    getBoundingClientRect: () => ({
      top: box.top,
      left: box.left,
      bottom: box.top + box.height,
      right: box.left + box.width,
      width: box.width,
      height: box.height,
      x: box.left,
      y: box.top,
      toJSON: () => ({}),
    }),
  } as HTMLElement;
}

describe("placeSlashMenuAnchor", () => {
  it("opens below when there is room", () => {
    const shell = fakeShell({ top: 100, left: 50, width: 600, height: 500 });
    const anchor = placeSlashMenuAnchor(
      { top: 140, bottom: 158, left: 80 },
      shell,
    );
    expect(anchor.top).toBeGreaterThanOrEqual(158);
    expect(anchor.maxHeight).toBeGreaterThan(140);
  });

  it("flips above when the caret is near the bottom", () => {
    const shell = fakeShell({ top: 100, left: 50, width: 600, height: 400 });
    const anchor = placeSlashMenuAnchor(
      { top: 450, bottom: 468, left: 80 },
      shell,
    );
    expect(anchor.top).toBeLessThan(450);
    expect(anchor.maxHeight).toBeLessThanOrEqual(320);
  });

  it("clamps left so the menu stays in the viewport", () => {
    const shell = fakeShell({ top: 0, left: 0, width: 280, height: 500 });
    const anchor = placeSlashMenuAnchor(
      { top: 40, bottom: 58, left: 260 },
      shell,
    );
    expect(anchor.left).toBeLessThan(280);
    expect(anchor.left).toBeGreaterThanOrEqual(8);
  });

  it("never sizes taller than available space below", () => {
    const shell = fakeShell({ top: 0, left: 0, width: 600, height: 200 });
    const anchor = placeSlashMenuAnchor(
      { top: 150, bottom: 168, left: 40 },
      shell,
    );
    expect(anchor.top + anchor.maxHeight).toBeLessThanOrEqual(200 - 8 + 1);
  });
});

describe("placeComposerSlashMenuAnchor", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("opens above when the input is near the bottom of the viewport", () => {
    vi.stubGlobal("innerHeight", 800);
    vi.stubGlobal("innerWidth", 1200);
    const anchor = placeComposerSlashMenuAnchor({
      top: 700,
      bottom: 740,
      left: 120,
    });
    expect(anchor.placement).toBe("above");
    expect(anchor.top + anchor.maxHeight).toBeLessThanOrEqual(700);
    expect(anchor.maxHeight).toBeGreaterThan(100);
  });

  it("keeps the menu inside the viewport when opening below", () => {
    vi.stubGlobal("innerHeight", 800);
    vi.stubGlobal("innerWidth", 1200);
    const anchor = placeComposerSlashMenuAnchor({
      top: 80,
      bottom: 120,
      left: 40,
    });
    expect(anchor.placement).toBe("below");
    expect(anchor.top + anchor.maxHeight).toBeLessThanOrEqual(800 - 8);
  });
});
