import { describe, expect, it } from "vitest";
import { shouldSubmitComposerKey } from "$lib/utils/composerKeyboard";

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
