<script lang="ts">
  import { isTauriMacDesktop } from "$lib/platform";
  import {
    composerMessageBoundary, handleComposerMessageNavigation,
    isComposerNavigationGlyph, moveComposerToMessageBoundary,
    stripComposerNavigationGlyphs,
  } from "$lib/utils/composerKeyboard";
  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    maxHeight?: number;
    minHeight?: number;
    element?: HTMLTextAreaElement | null;
    onkeydown?: (event: KeyboardEvent) => void;
    onblur?: (event: FocusEvent) => void;
    onfocus?: (event: FocusEvent) => void;
    oninput?: (event: Event) => void;
    onclick?: (event: MouseEvent) => void;
    onkeyup?: (event: KeyboardEvent) => void;
    onpaste?: (event: ClipboardEvent) => void;
    onselect?: (event: Event) => void;
    enterkeyhint?: "enter" | "done" | "go" | "next" | "previous" | "search" | "send";
    "aria-label"?: string;
  }

  let {
    value = $bindable(""),
    placeholder = "",
    disabled = false,
    class: className = "",
    maxHeight = 128,
    minHeight = 36,
    element = $bindable<HTMLTextAreaElement | null>(null),
    onkeydown,
    onblur,
    onfocus,
    oninput,
    onclick,
    onkeyup,
    onselect,
    onpaste,
    enterkeyhint,
    "aria-label": ariaLabel,
  }: Props = $props();

  let preserveNativeNavigation = false;
  let navigationInputSnapshot: {
    text: string;
    start: number;
    end: number;
    direction: "forward" | "backward" | "none";
  } | null = null;

  function resize() {
    if (!element) return;
    element.style.height = "0px";
    const scroll = element.scrollHeight;
    const height = Math.min(Math.max(scroll, minHeight), maxHeight);
    element.style.height = `${height}px`;
    element.style.overflowY = scroll > maxHeight ? "auto" : "hidden";
    element.dataset.expanded = scroll > minHeight + 6 ? "true" : "false";
  }

  function scheduleResize() {
    // Let bind:value finish before measuring scrollHeight. This matters for
    // the fresh-chat presence composer, where the draft is a shared store
    // value and the input event can otherwise measure the previous value.
    queueMicrotask(resize);
  }

  function handleKeydown(event: KeyboardEvent) {
    onkeydown?.(event);
    preserveNativeNavigation = event.defaultPrevented || event.shiftKey ||
      event.ctrlKey || event.metaKey || event.altKey || event.isComposing;
    if (element && isTauriMacDesktop()) {
      handleComposerMessageNavigation(event, element);
    }
  }

  function handleKeyup(event: KeyboardEvent) {
    preserveNativeNavigation = false;
    onkeyup?.(event);
  }

  function handleBeforeInput(event: InputEvent) {
    if (!element || !isTauriMacDesktop() || event.isComposing ||
        event.inputType !== "insertText" || !isComposerNavigationGlyph(event.data)) return;
    if (!event.cancelable) {
      navigationInputSnapshot = {
        text: element.value, start: element.selectionStart, end: element.selectionEnd,
        direction: element.selectionDirection,
      };
      return;
    }
    // Some AppKit events reach WebKit as text without a useful keydown. Do not
    // insert a control into the draft (or its undo history) just to strip it later.
    event.preventDefault();
    const boundary = composerMessageBoundary(event.data);
    if (boundary && !preserveNativeNavigation) moveComposerToMessageBoundary(element, boundary);
  }

  function handleInput(event: Event) {
    if (element) {
      // A leaked character can replace selected text. Restore the pre-input
      // snapshot, rather than only removing the glyph and losing that selection.
      if (navigationInputSnapshot && event instanceof InputEvent &&
          event.inputType === "insertText" && isComposerNavigationGlyph(event.data)) {
        const snapshot = navigationInputSnapshot;
        element.value = snapshot.text;
        value = snapshot.text;
        element.setSelectionRange(snapshot.start, snapshot.end, snapshot.direction);
      }
      navigationInputSnapshot = null;
      const raw = element.value;
      const clean = stripComposerNavigationGlyphs(raw);
      if (clean !== raw) {
        const start = element.selectionStart;
        const end = element.selectionEnd;
        const cleanStart = stripComposerNavigationGlyphs(raw.slice(0, start)).length;
        const cleanEnd = stripComposerNavigationGlyphs(raw.slice(0, end)).length;
        element.value = clean;
        value = clean;
        element.setSelectionRange(cleanStart, cleanEnd, element.selectionDirection);
      }
      // Retain the same navigation fallback if WebKit emits noncancelable input.
      if (isTauriMacDesktop() && event instanceof InputEvent &&
          !event.isComposing && !preserveNativeNavigation && event.inputType === "insertText") {
        const boundary = composerMessageBoundary(event.data);
        if (boundary) moveComposerToMessageBoundary(element, boundary);
      }
    }
    scheduleResize();
    oninput?.(event);
  }

  $effect(() => {
    value;
    scheduleResize();
  });
</script>

<textarea
  bind:this={element}
  bind:value
  {placeholder}
  {disabled}
  onkeydown={handleKeydown}
  onbeforeinput={handleBeforeInput}
  {onblur}
  {onfocus}
  {onclick}
  onkeyup={handleKeyup}
  {onselect}
  {onpaste}
  {enterkeyhint}
  aria-label={ariaLabel}
  rows="1"
  class="composer-bar-input {className}"
  style={`max-height: ${maxHeight}px`}
  oninput={handleInput}
></textarea>
