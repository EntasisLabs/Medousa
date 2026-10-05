import { describe, expect, it } from "vitest";
import { handleComposerMessageNavigation, shouldSubmitComposerKey, stripComposerNavigationGlyphs } from "$lib/utils/composerKeyboard";

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

describe("composer navigation key decoding", () => {
  it("recovers Home/End when WebKit reports an unidentified key with its physical code", () => {
    const element = { value: "message", setSelectionRange: () => {}, scrollHeight: 100, scrollTop: 0 } as unknown as HTMLTextAreaElement;
    const event = { key: "Unidentified", code: "End", shiftKey: false, ctrlKey: false,
      altKey: false, metaKey: false, isComposing: false, defaultPrevented: false,
      preventDefault: () => {} };
    expect(handleComposerMessageNavigation(event, element)).toBe(true);
    expect(element.scrollTop).toBe(100);
    // A printable remapped key must remain text even if its physical code is Home.
    expect(handleComposerMessageNavigation({ ...event, key: "h", code: "Home" }, element)).toBe(false);
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
