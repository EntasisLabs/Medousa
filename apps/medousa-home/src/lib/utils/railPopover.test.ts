import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  placeComposerPopover,
  placeRailPopover,
  placeToolbarPopover,
  railPopoverOpenHeightCap,
  resolveRailPopoverExpand,
} from "./railPopover";

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

  it("resolves expand down when there is useful space below", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 120, left: 20, right: 200, bottom: 156, width: 180, height: 36 }),
    } as HTMLElement;
    expect(resolveRailPopoverExpand(trigger, { pad: 8 })).toBe("down");
  });

  it("resolves expand up near the bottom of the viewport", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 720, left: 20, right: 200, bottom: 756, width: 180, height: 36 }),
    } as HTMLElement;
    expect(resolveRailPopoverExpand(trigger, { pad: 8 })).toBe("up");
  });

  it("caps open height to available space on the expand side", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 700, left: 20, right: 200, bottom: 736, width: 180, height: 36 }),
    } as HTMLElement;
    const down = railPopoverOpenHeightCap(trigger, "down", { pad: 8, maxHeight: 512 });
    const up = railPopoverOpenHeightCap(trigger, "up", { pad: 8, maxHeight: 900 });
    expect(down).toBe(800 - 8 - 700); // 92 — room below trigger top
    expect(up).toBe(736 - 8); // 728 — room above trigger bottom
    expect(up).toBeGreaterThan(down);
    expect(
      railPopoverOpenHeightCap(trigger, "up", { pad: 8, maxHeight: 200 }),
    ).toBe(200);
  });

  it("pins top when expanding down from the trigger", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 200, left: 20, right: 200, bottom: 236, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 40 });

    placeRailPopover(trigger, menu, {
      alignY: "start",
      expand: "down",
      openHeight: 400,
      pad: 8,
    });

    expect(menu.style.top).toBe("200px");
    expect(menu.style.bottom).toBe("auto");
    expect(menu.style.maxHeight).toBe("400px");
  });

  it("pins bottom when expanding up from the trigger", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 720, left: 20, right: 200, bottom: 756, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 40 });

    placeRailPopover(trigger, menu, {
      alignY: "start",
      expand: "up",
      openHeight: 400,
      pad: 8,
    });

    expect(menu.style.top).toBe("auto");
    expect(menu.style.bottom).toBe("44px"); // 800 - 756
    expect(menu.style.maxHeight).toBe("400px");
  });

  it("keeps bottom locked while expanding up", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 720, left: 20, right: 200, bottom: 756, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 280, height: 40 });
    menu.style.bottom = "44px";

    placeRailPopover(trigger, menu, {
      alignY: "start",
      expand: "up",
      openHeight: 400,
      lockEdge: "bottom",
      pad: 8,
    });

    expect(menu.style.bottom).toBe("44px");
    expect(menu.style.top).toBe("auto");
  });

  it("places to the right of the cursor instead of docking beside the rail", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 200, left: 20, right: 200, bottom: 236, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 120, height: 40 });

    placeRailPopover(trigger, menu, {
      alignY: "start",
      expand: "down",
      openHeight: 400,
      cursor: { x: 400, y: 300 },
      cursorGap: 12,
      pad: 8,
    });

    // Right of cursor: 400 + 12 = 412
    expect(menu.style.left).toBe("412px");
    // Vertically centered on cursor: 300 - 40/2 = 280
    expect(menu.style.top).toBe("280px");
    // Not rail-docked (would have been 200 + gap)
    expect(menu.style.left).not.toBe("208px");
  });

  it("keeps cursor-right placement when expanding up", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 720, left: 20, right: 200, bottom: 756, width: 180, height: 36 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 120, height: 40 });

    placeRailPopover(trigger, menu, {
      alignY: "start",
      expand: "up",
      openHeight: 400,
      cursor: { x: 400, y: 740 },
      cursorGap: 12,
      pad: 8,
    });

    // menuBottom = 740 + 20 = 760 → bottom inset = 800 - 760 = 40
    expect(menu.style.top).toBe("auto");
    expect(menu.style.bottom).toBe("40px");
    expect(menu.style.left).toBe("412px");
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

describe("placeToolbarPopover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", {
      innerWidth: 1200,
      innerHeight: 800,
      visualViewport: undefined,
    });
  });

  it("opens above a dock trigger near the bottom", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 760, left: 300, right: 332, bottom: 792, width: 32, height: 32 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 352, height: 420 });
    (menu as HTMLElement & { scrollHeight: number }).scrollHeight = 420;

    placeToolbarPopover(trigger, menu, { prefer: "above", width: 352 });

    const top = Number.parseInt(menu.style.top, 10);
    const left = Number.parseInt(menu.style.left, 10);
    const maxH = Number.parseInt(menu.style.maxHeight, 10);
    expect(top + Math.min(420, maxH)).toBeLessThanOrEqual(760);
    expect(left + 352).toBeLessThanOrEqual(1188);
    expect(maxH).toBeLessThanOrEqual(760 - 12 - 6);
  });

  it("opens below a titlebar trigger when there is room", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 48, left: 900, right: 932, bottom: 80, width: 32, height: 32 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 352, height: 200 });

    placeToolbarPopover(trigger, menu, { prefer: "below", width: 352 });

    expect(Number.parseInt(menu.style.top, 10)).toBe(80 + 6);
  });

  it("caps height to available space above a bottom dock", () => {
    const trigger = {
      getBoundingClientRect: () =>
        fakeRect({ top: 700, left: 300, right: 332, bottom: 732, width: 32, height: 32 }),
    } as HTMLElement;
    const menu = fakeMenu({ width: 352, height: 900 });

    placeToolbarPopover(trigger, menu, { prefer: "above", width: 352 });

    const maxH = Number.parseInt(menu.style.maxHeight, 10);
    const top = Number.parseInt(menu.style.top, 10);
    expect(maxH).toBeLessThanOrEqual(700 - 12 - 6);
    expect(top).toBeGreaterThanOrEqual(12);
    expect(top + Math.min(900, maxH)).toBeLessThanOrEqual(800 - 12);
  });
});
