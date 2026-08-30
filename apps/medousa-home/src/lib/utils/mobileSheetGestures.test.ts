/** @vitest-environment happy-dom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

function touchEvent(type: string, x: number, y: number): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  const touch = { clientX: x, clientY: y };
  Object.defineProperty(event, "touches", {
    value: type === "touchend" ? [] : [touch],
  });
  Object.defineProperty(event, "changedTouches", { value: [touch] });
  return event;
}

function swipe(header: HTMLElement, fromY: number, toY: number, x = 120) {
  header.dispatchEvent(touchEvent("touchstart", x, fromY));
  header.dispatchEvent(touchEvent("touchmove", x, toY));
  header.dispatchEvent(touchEvent("touchend", x, toY));
}

describe("attachMobileSheetGestures", () => {
  let sheet: HTMLDivElement;
  let header: HTMLElement;

  beforeEach(() => {
    vi.useFakeTimers();
    sheet = document.createElement("div");
    header = document.createElement("header");
    sheet.append(header);
    document.body.append(sheet);
    vi.spyOn(sheet, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 448,
      top: 448,
      right: 320,
      bottom: 768,
      left: 0,
      width: 320,
      height: 320,
      toJSON: () => ({}),
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  it("expands upward, collapses downward, then dismisses from the resting height", () => {
    const onDismiss = vi.fn();
    const cleanup = attachMobileSheetGestures(sheet, header, { onDismiss });

    swipe(header, 300, 220);
    expect(sheet.classList.contains("mobile-sheet-expanded")).toBe(true);
    expect(sheet.dataset.sheetExpanded).toBe("true");
    expect(onDismiss).not.toHaveBeenCalled();

    swipe(header, 220, 300);
    expect(sheet.classList.contains("mobile-sheet-expanded")).toBe(false);
    expect(sheet.dataset.sheetExpanded).toBeUndefined();
    expect(onDismiss).not.toHaveBeenCalled();

    swipe(header, 300, 380);
    expect(onDismiss).toHaveBeenCalledOnce();

    cleanup();
  });

  it("returns to its resting size when an upward drag misses the threshold", () => {
    const cleanup = attachMobileSheetGestures(sheet, header, {
      onDismiss: vi.fn(),
    });

    swipe(header, 300, 270);
    expect(sheet.classList.contains("mobile-sheet-expanded")).toBe(false);
    expect(sheet.dataset.sheetExpanded).toBeUndefined();

    vi.runAllTimers();
    expect(sheet.style.height).toBe("");
    expect(sheet.style.maxHeight).toBe("");
    cleanup();
  });

  it("does not start a sheet gesture from an interactive header control", () => {
    const onDismiss = vi.fn();
    const button = document.createElement("button");
    header.append(button);
    const cleanup = attachMobileSheetGestures(sheet, header, { onDismiss });

    swipe(button, 300, 220);
    expect(sheet.classList.contains("mobile-sheet-expanded")).toBe(false);
    expect(onDismiss).not.toHaveBeenCalled();
    cleanup();
  });

  it("can opt a non-resizable surface out of expansion", () => {
    const cleanup = attachMobileSheetGestures(sheet, header, {
      onDismiss: vi.fn(),
      expandable: false,
    });

    swipe(header, 300, 220);
    expect(sheet.classList.contains("mobile-sheet-expanded")).toBe(false);
    cleanup();
  });
});
