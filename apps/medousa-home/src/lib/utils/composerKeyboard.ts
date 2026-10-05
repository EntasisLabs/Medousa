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

type ComposerNavigationKeyEvent = Pick<KeyboardEvent,
  "key" | "code" | "shiftKey" | "ctrlKey" | "metaKey" | "altKey" |
  "isComposing" | "defaultPrevented" | "preventDefault"
>;

export function composerMessageBoundary(key: string | null): "start" | "end" | null {
  if (key === "Home" || key === "\uF729") return "start";
  if (key === "End" || key === "\uF72B") return "end";
  return null;
}

export function moveComposerToMessageBoundary(
  element: HTMLTextAreaElement,
  boundary: "start" | "end",
): void {
  const position = boundary === "start" ? 0 : element.value.length;
  element.setSelectionRange(position, position);
  element.scrollTop = boundary === "start" ? 0 : element.scrollHeight;
}

/**
 * Native macOS Home/End may arrive as AppKit function-key text. Handle the
 * unmodified keys before insertion; keep native selection/IME shortcuts intact.
 * Call only from the Mac desktop shell, where Shift+Home/End selects to these
 * document boundaries. Other platforms retain their native line navigation.
 */
export function handleComposerMessageNavigation(
  event: ComposerNavigationKeyEvent,
  element: HTMLTextAreaElement,
): boolean {
  if (event.defaultPrevented || event.isComposing || event.shiftKey ||
      event.ctrlKey || event.metaKey || event.altKey) return false;
  const boundary = composerMessageBoundary(event.key) ??
    (event.key === "Unidentified" ? composerMessageBoundary(event.code) : null);
  if (!boundary) return false;
  event.preventDefault();
  moveComposerToMessageBoundary(element, boundary);
  return true;
}

/** Only a standalone navigation control, never an ordinary text/IME fragment. */
export function isComposerNavigationGlyph(value: string | null): boolean {
  return value !== null && /^[\u001C-\u001F\uF700-\uF747]$/.test(value);
}

/**
 * WebKit can surface AppKit navigation keys as private-use characters during
 * text navigation (NSUpArrowFunctionKey starts at U+F700). They are controls,
 * not user text, and otherwise render as empty square glyphs in the composer.
 */
export function stripComposerNavigationGlyphs(value: string): string {
  return value.replace(/[\u001C-\u001F\uF700-\uF747]/g, "");
}
