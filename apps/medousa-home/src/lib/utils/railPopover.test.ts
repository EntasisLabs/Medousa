import { beforeEach, describe, expect, it, vi } from "vitest";
import { placeComposerPopover, placeRailPopover } from "./railPopover";

function fakeRect(partial: Partial<DOMRect>): DOMRect {
  return {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
    toJSON: () => ({}),
    ...partial,
  } as DOMRect;
}

function fakeMenu(size: { width: number; height: number }) {
  return {
    getBoundingClientRect: () =>
      fakeRect({
        top: 0,
        left: 0,
        right: size.width,
        bottom: size.height,
        width: size.width,
        height: size.height,
      }),
    offsetWidth: size.width,
    offsetHeight: size.height,
    style: {
      top: "",
      left: "",
      maxWidth: "",
      maxHeight: "",
      position: "",
    } as Record<string, string>,
  } as unknown as HTMLElement & { style: Record<string, string> };
}

describe("placeRailPopover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", {
      innerWidth: 1200,
      innerHeight: 800,
      visualViewport: undefined,
    });
  });

  it("places the menu to the right of the trigger", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 400, left: 20, right: 200, bottom: 436, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 200 });

    placeRailPopover(trigger, menu);

    expect(menu.style.left).toBe("208px");
    const top = Number.parseInt(menu.style.top, 10);
    const bottom = top + 200;
    expect(top).toBeGreaterThanOrEqual(8);
    expect(bottom).toBeLessThanOrEqual(792);
  });

  it("clamps vertically when near the bottom of the viewport", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 760, left: 20, right: 200, bottom: 796, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 320 });

    placeRailPopover(trigger, menu);

    const top = Number.parseInt(menu.style.top, 10);
    expect(top + 320).toBeLessThanOrEqual(792);
    expect(top).toBeGreaterThanOrEqual(8);
  });

  it("clamps vertically when near the top of the viewport", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 4, left: 20, right: 200, bottom: 40, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 300 });

    placeRailPopover(trigger, menu);

    const top = Number.parseInt(menu.style.top, 10);
    expect(top).toBeGreaterThanOrEqual(8);
    expect(top + 300).toBeLessThanOrEqual(792);
  });

  it("flips left when the right side would overflow", () => {
    vi.stubGlobal("window", {
      innerWidth: 360,
      innerHeight: 800,
      visualViewport: undefined,
    });
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 200, left: 40, right: 200, bottom: 236, width: 160, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 200 });

    placeRailPopover(trigger, menu);

    const left = Number.parseInt(menu.style.left, 10);
    expect(left).toBeGreaterThanOrEqual(8);
    expect(left + 280).toBeLessThanOrEqual(352);
  });

  it("caps oversized menus to the viewport", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 100, left: 20, right: 200, bottom: 136, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 2000, height: 2000 });

    placeRailPopover(trigger, menu);

    expect(menu.style.maxWidth).toBe("1184px");
    expect(menu.style.maxHeight).toBe("784px");
    const left = Number.parseInt(menu.style.left, 10);
    const top = Number.parseInt(menu.style.top, 10);
    expect(left).toBe(8);
    expect(top).toBe(8);
  });
});

describe("placeComposerPopover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", {
      innerWidth: 1200,
      innerHeight: 800,
      visualViewport: undefined,
    });
  });

  it("places the menu above the trigger", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 600, left: 40, right: 80, bottom: 636, width: 40, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 200 });

    placeComposerPopover(trigger, menu);

    expect(menu.style.position).toBe("fixed");
    expect(Number.parseInt(menu.style.top, 10)).toBe(600 - 8 - 200);
    expect(Number.parseInt(menu.style.left, 10)).toBe(40);
  });

  it("flips below when there is not enough room above", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 40, left: 40, right: 80, bottom: 76, width: 40, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 200 });

    placeComposerPopover(trigger, menu);

    expect(Number.parseInt(menu.style.top, 10)).toBe(76 + 8);
  });
});
