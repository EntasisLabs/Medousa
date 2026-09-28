import { describe, expect, it } from "vitest";
import { shouldSubmitComposerKey, stripComposerNavigationGlyphs } from "$lib/utils/composerKeyboard";

function keyEvent(
  overrides: Partial<Pick<KeyboardEvent, "key" | "shiftKey" | "isComposing">> = {},
) {
  return {
    key: "Enter",
    shiftKey: false,
    isComposing: false,
    ...overrides,
  };
}

describe("shouldSubmitComposerKey", () => {
  it("submits plain Enter on desktop", () => {
    expect(shouldSubmitComposerKey(keyEvent(), false)).toBe(true);
  });

  it("keeps Enter as a newline on mobile", () => {
    expect(shouldSubmitComposerKey(keyEvent(), true)).toBe(false);
  });

  it("does not submit Shift+Enter or an IME composition", () => {
    expect(
      shouldSubmitComposerKey(keyEvent({ shiftKey: true }), false),
    ).toBe(false);
    expect(
      shouldSubmitComposerKey(keyEvent({ isComposing: true }), false),
    ).toBe(false);
  });
});


describe("stripComposerNavigationGlyphs", () => {
  it("removes AppKit arrow and text-navigation key glyphs", () => {
    expect(stripComposerNavigationGlyphs(`before\uF702middle\uF703after`)).toBe(
      "beforemiddleafter",
    );
    expect(stripComposerNavigationGlyphs(`up\uF700down\uF701`)).toBe("updown");
  });

  it("preserves ordinary text, emoji, and typographic arrows", () => {
    const text = "hello → world 👋";
    expect(stripComposerNavigationGlyphs(text)).toBe(text);
  });
});
