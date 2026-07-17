import { describe, expect, it, vi } from "vitest";
import { placeSlashMenuAnchor } from "./slashMenuPlacement";

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
    expect(anchor.top).toBeDefined();
    expect(anchor.bottom).toBeUndefined();
    expect(anchor.top).toBeGreaterThan(58);
    expect(anchor.maxHeight).toBeGreaterThan(140);
  });

  it("flips above when the caret is near the bottom", () => {
    const shell = fakeShell({ top: 100, left: 50, width: 600, height: 400 });
    const anchor = placeSlashMenuAnchor(
      { top: 450, bottom: 468, left: 80 },
      shell,
    );
    expect(anchor.bottom).toBeDefined();
    expect(anchor.top).toBeUndefined();
    expect(anchor.maxHeight).toBeLessThanOrEqual(320);
  });

  it("clamps left so the menu stays in the shell", () => {
    const shell = fakeShell({ top: 0, left: 0, width: 280, height: 500 });
    const anchor = placeSlashMenuAnchor(
      { top: 40, bottom: 58, left: 260 },
      shell,
    );
    expect(anchor.left).toBeLessThan(280);
    expect(anchor.left).toBeGreaterThanOrEqual(8);
  });
});
