import { isTauriMacDesktop } from "$lib/platform";
import {
  composerMessageBoundary, handleComposerMessageNavigation,
  isComposerNavigationGlyph, moveComposerToMessageBoundary,
  stripComposerNavigationGlyphs, textNavigationKey,
} from "./composerKeyboard";

/** Apply the composer's AppKit input protection to single-line search/prompt fields. */
export function guardTextNavigation(element: HTMLInputElement) {
  let keydown: KeyboardEvent | null = null;
  let snapshot: { text: string; start: number; end: number; direction: "forward" | "backward" | "none" } | null = null;
  const preserveNative = () => Boolean(keydown && (keydown.defaultPrevented || keydown.isComposing ||
    keydown.shiftKey || keydown.ctrlKey || keydown.metaKey || keydown.altKey));

  function onKeydown(event: KeyboardEvent) {
    keydown = event;
    if (!isTauriMacDesktop()) return;
    handleComposerMessageNavigation(event, element);
    // Native DOM arrows already move the caret. Only repair leaked AppKit keys;
    // Up/Down remain available to Spotlight's result / destination navigation.
    const key = textNavigationKey(event);
    if (key === event.key || preserveNative() || (key !== "ArrowLeft" && key !== "ArrowRight")) return;
    event.preventDefault();
    const start = element.selectionStart ?? 0;
    const end = element.selectionEnd ?? start;
    const boundaries = [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(element.value)]
      .map(segment => segment.index);
    boundaries.push(element.value.length);
    const position = key === "ArrowLeft"
      ? (start !== end ? start : boundaries.filter(index => index < start).at(-1) ?? 0)
      : (start !== end ? end : boundaries.find(index => index > end) ?? end);
    element.setSelectionRange(position, position);
  }
  function reset() { keydown = null; snapshot = null; }
  function onBeforeInput(event: InputEvent) {
    snapshot = null;
    if (!isTauriMacDesktop() || event.isComposing || event.inputType !== "insertText" ||
        !isComposerNavigationGlyph(event.data)) return;
    if (!event.cancelable) {
      snapshot = { text: element.value, start: element.selectionStart ?? 0,
        end: element.selectionEnd ?? 0, direction: element.selectionDirection ?? "none" };
      return;
    }
    event.preventDefault();
    const boundary = composerMessageBoundary(event.data);
    if (boundary && !preserveNative()) moveComposerToMessageBoundary(element, boundary);
  }
  function onInput(event: Event) {
    if (!isTauriMacDesktop() || (event instanceof InputEvent && event.isComposing)) {
      snapshot = null;
      return;
    }
    if (snapshot && event instanceof InputEvent && event.inputType === "insertText" &&
        isComposerNavigationGlyph(event.data)) {
      element.value = snapshot.text;
      element.setSelectionRange(snapshot.start, snapshot.end, snapshot.direction);
    }
    snapshot = null;
    const raw = element.value;
    const clean = stripComposerNavigationGlyphs(raw);
    if (clean !== raw) {
      const start = stripComposerNavigationGlyphs(raw.slice(0, element.selectionStart ?? 0)).length;
      const end = stripComposerNavigationGlyphs(raw.slice(0, element.selectionEnd ?? 0)).length;
      const direction = element.selectionDirection;
      element.value = clean;
      element.setSelectionRange(start, end, direction ?? "none");
    }
    if (event instanceof InputEvent && event.inputType === "insertText" && !preserveNative()) {
      const boundary = composerMessageBoundary(event.data);
      if (boundary) moveComposerToMessageBoundary(element, boundary);
    }
  }
  // Capture before Svelte's value binding: consumers must never see a dirty query.
  element.addEventListener("keydown", onKeydown);
  element.addEventListener("keyup", reset);
  element.addEventListener("blur", reset);
  element.addEventListener("beforeinput", onBeforeInput);
  element.addEventListener("input", onInput, true);
  return { destroy() {
    element.removeEventListener("keydown", onKeydown);
    element.removeEventListener("keyup", reset);
    element.removeEventListener("blur", reset);
    element.removeEventListener("beforeinput", onBeforeInput);
    element.removeEventListener("input", onInput, true);
  } };
}
