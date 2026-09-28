type ComposerKeyEvent = Pick<KeyboardEvent, "key" | "shiftKey" | "isComposing">;

/** Desktop Enter sends; mobile Enter remains the textarea's newline action. */
export function shouldSubmitComposerKey(
  event: ComposerKeyEvent,
  mobile: boolean,
): boolean {
  return (
    !mobile &&
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.isComposing
  );
}

/**
 * WebKit can surface AppKit navigation keys as private-use characters during
 * text navigation (NSUpArrowFunctionKey starts at U+F700). They are controls,
 * not user text, and otherwise render as empty square glyphs in the composer.
 */
export function stripComposerNavigationGlyphs(value: string): string {
  return value.replace(/[\u001C-\u001F\uF700-\uF747]/g, "");
}
