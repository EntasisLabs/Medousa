/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { guardTextNavigation } from "./guardTextNavigation";

const platform = vi.hoisted(() => ({ mac: true }));
vi.mock("$lib/platform", () => ({ isTauriMacDesktop: () => platform.mac }));
let dispose: (() => void) | undefined;
function setup() {
  const input = document.createElement("input");
  input.value = "hello 👩🏽‍💻 world";
  document.body.append(input);
  input.focus(); input.setSelectionRange(5, 5);
  dispose = guardTextNavigation(input).destroy;
  return input;
}
function key(input: HTMLInputElement, key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  input.dispatchEvent(event);
  return event;
}
function before(input: HTMLInputElement, data: string, options: InputEventInit = {}) {
  const event = new InputEvent("beforeinput", { data, inputType: "insertText", cancelable: true, ...options });
  input.dispatchEvent(event);
  return event;
}
afterEach(() => { dispose?.(); document.body.replaceChildren(); platform.mac = true; });

it.each(["\uF700", "\uF701", "\uF702", "\uF703", "\uF729", "\uF72B", "\uF72C", "\uF72D", "\u001C"])(
  "blocks AppKit control %s before it replaces selected search text", glyph => {
    const input = setup(); input.setSelectionRange(0, 5);
    expect(before(input, glyph).defaultPrevented).toBe(true);
    expect(input.value).toBe("hello 👩🏽‍💻 world");
  },
);
it.each(["Home", "\uF729", "End", "\uF72B"])("moves the caret for %s without changing text", value => {
  const input = setup(); const original = input.value;
  expect(key(input, value).defaultPrevented).toBe(true);
  expect(input.selectionStart).toBe(value === "Home" || value === "\uF729" ? 0 : original.length);
  expect(input.value).toBe(original);
});
it("keeps leaked left/right caret movement on grapheme boundaries", () => {
  const input = setup(); input.setSelectionRange(6, 6);
  key(input, "\uF703"); expect(input.selectionStart).toBe("hello 👩🏽‍💻".length);
  key(input, "\uF702"); expect(input.selectionStart).toBe(6);
  input.setSelectionRange(0, 5);
  key(input, "\uF703"); expect(input.selectionStart).toBe(5);
});
it("restores selected text after noncancelable insertion before value-binding listeners run", () => {
  const input = setup(); const original = input.value;
  input.setSelectionRange(0, 5);
  before(input, "\uF72B", { cancelable: false });
  input.value = "\uF72B" + original.slice(5); input.setSelectionRange(1, 1);
  let observed = "";
  input.addEventListener("input", () => { observed = input.value; });
  input.dispatchEvent(new InputEvent("input", { data: "\uF72B", inputType: "insertText" }));
  expect(observed).toBe(original);
  expect(input.selectionStart).toBe(original.length);
});
it("cleans repeated leaked controls even without beforeinput, preserving the caret", () => {
  const input = setup(); input.value = "sd\uF700\uF701\uF72Bafter"; input.setSelectionRange(5, 5);
  input.dispatchEvent(new Event("input"));
  expect(input.value).toBe("sdafter"); expect(input.selectionStart).toBe(2);
});
it("preserves modified native selections while rejecting their leaked text", () => {
  const input = setup(); key(input, "Home", { shiftKey: true });
  input.setSelectionRange(0, 5, "backward");
  expect(before(input, "\uF729").defaultPrevented).toBe(true);
  expect(input.selectionStart).toBe(0); expect(input.selectionEnd).toBe(5);
  expect(input.selectionDirection).toBe("backward");
});
it("leaves normal navigation, shortcuts, Unicode and composition to the native editor", () => {
  const input = setup();
  for (const value of ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PageDown"]) {
    expect(key(input, value).defaultPrevented).toBe(false);
  }
  expect(key(input, "End", { metaKey: true }).defaultPrevented).toBe(false);
  expect(key(input, "End", { isComposing: true }).defaultPrevented).toBe(false);
  for (const data of ["hello", "→", "👋", "日本語", "\uE000"]) expect(before(input, data).defaultPrevented).toBe(false);
  expect(before(input, "\uF729", { inputType: "insertCompositionText", isComposing: true }).defaultPrevented).toBe(false);
});
it("retains native behavior outside macOS and removes listeners on destruction", () => {
  const input = setup(); platform.mac = false;
  expect(key(input, "End").defaultPrevented).toBe(false);
  expect(before(input, "\uF72B").defaultPrevented).toBe(false);
  platform.mac = true; dispose?.();
  expect(before(input, "\uF72B").defaultPrevented).toBe(false);
});
