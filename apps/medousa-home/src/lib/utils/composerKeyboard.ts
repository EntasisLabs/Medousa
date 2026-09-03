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
