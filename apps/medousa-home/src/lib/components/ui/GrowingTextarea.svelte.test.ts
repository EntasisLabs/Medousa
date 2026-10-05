/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import GrowingTextarea from "./GrowingTextarea.svelte";

const platform = vi.hoisted(() => ({ nativeMac: true }));
vi.mock("$lib/platform", () => ({ isTauriMacDesktop: () => platform.nativeMac }));
let component: ReturnType<typeof mount> | undefined;

function composer(onkeydown = vi.fn<(event: KeyboardEvent) => void>()) {
  const draft = writable("first line\nlast line");
  const state = fromStore(draft);
  component = mount(GrowingTextarea, {
    target: document.body,
    props: {
      get value() { return state.current; },
      set value(text: string) { draft.set(text); },
      onkeydown,
    },
  });
  flushSync();
  const textarea = document.querySelector("textarea")!;
  textarea.focus();
  textarea.setSelectionRange(5, 5);
  return { textarea, state, onkeydown };
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
  platform.nativeMac = true;
});

it.each(["Home", "\uF729", "End", "\uF72B"])(
  "moves the Mac composer caret for %s without inserting or changing the draft", (key) => {
    const { textarea, state, onkeydown } = composer();
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    textarea.dispatchEvent(event);
    const position = key === "Home" || key === "\uF729" ? 0 : textarea.value.length;
    expect(event.defaultPrevented).toBe(true);
    expect(textarea.selectionStart).toBe(position);
    expect(textarea.selectionEnd).toBe(position);
    expect(state.current).toBe("first line\nlast line");
    expect(onkeydown).toHaveBeenCalledOnce();
  },
);

it.each(["shiftKey", "ctrlKey", "metaKey", "altKey", "isComposing"])(
  "leaves native %s navigation and composition alone", (modifier) => {
    const { textarea, onkeydown } = composer();
    const event = new KeyboardEvent("keydown", { key: "End", [modifier]: true, bubbles: true, cancelable: true });
    textarea.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(onkeydown).toHaveBeenCalledOnce();
    expect(textarea.selectionStart).toBe(5);
  },
);

it("preserves a key handled by the consumer", () => {
  const { textarea } = composer(vi.fn((event) => event.preventDefault()));
  textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true, cancelable: true }));
  expect(textarea.selectionStart).toBe(5);
});

it("retains native Home/End behavior outside the Mac desktop shell", () => {
  platform.nativeMac = false;
  const { textarea } = composer();
  const event = new KeyboardEvent("keydown", { key: "End", bubbles: true, cancelable: true });
  textarea.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(false);
  expect(textarea.selectionStart).toBe(5);
});

it("handles text-only AppKit Home/End before they enter the draft or undo history", () => {
  const { textarea, state } = composer();
  for (const [data, position] of [["\uF72B", textarea.value.length], ["\uF729", 0]] as const) {
    const event = new InputEvent("beforeinput", { data, inputType: "insertText", bubbles: true, cancelable: true });
    textarea.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(textarea.selectionStart).toBe(position);
    expect(textarea.selectionEnd).toBe(position);
    expect(state.current).toBe("first line\nlast line");
  }
});

it("moves the caret and restores the bound draft after noncancelable navigation insertion", () => {
  const { textarea, state } = composer();
  const original = textarea.value;
  textarea.value = original.slice(0, 5) + "\uF72B" + original.slice(5);
  textarea.setSelectionRange(6, 6);
  textarea.dispatchEvent(new InputEvent("input", { data: "\uF72B", inputType: "insertText", bubbles: true }));
  flushSync();
  expect(textarea.value).toBe(original);
  expect(state.current).toBe(original);
  expect(textarea.selectionStart).toBe(original.length);
});

it("does not intercept ordinary text, paste, or active IME insertion", () => {
  const { textarea } = composer();
  for (const init of [
    { data: "a", inputType: "insertText" },
    { data: "\uF729", inputType: "insertFromPaste" },
    { data: "\uF729", inputType: "insertCompositionText", isComposing: true },
  ]) {
    const event = new InputEvent("beforeinput", { ...init, bubbles: true, cancelable: true });
    textarea.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(textarea.selectionStart).toBe(5);
  }
});

it("preserves native Shift selection if AppKit also sends a navigation glyph", () => {
  const { textarea } = composer();
  textarea.dispatchEvent(new KeyboardEvent("keydown", {
    key: "Home", shiftKey: true, bubbles: true, cancelable: true,
  }));
  // Simulate the native editor's working Shift+Home selection.
  textarea.setSelectionRange(0, 5, "backward");
  const event = new InputEvent("beforeinput", {
    data: "\uF729", inputType: "insertText", bubbles: true, cancelable: true,
  });
  textarea.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(textarea.selectionStart).toBe(0);
  expect(textarea.selectionEnd).toBe(5);
  expect(textarea.selectionDirection).toBe("backward");
});

it("does not lose selected draft text to noncancelable Home/End input", () => {
  const { textarea, state } = composer();
  const original = textarea.value;
  textarea.setSelectionRange(0, 5);
  textarea.dispatchEvent(new InputEvent("beforeinput", {
    data: "\uF72B", inputType: "insertText", bubbles: true,
  }));
  textarea.value = "\uF72B" + original.slice(5);
  textarea.setSelectionRange(1, 1);
  textarea.dispatchEvent(new InputEvent("input", {
    data: "\uF72B", inputType: "insertText", bubbles: true,
  }));
  flushSync();
  expect(textarea.value).toBe(original);
  expect(state.current).toBe(original);
  expect(textarea.selectionStart).toBe(original.length);
});
