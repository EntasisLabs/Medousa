/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { createPetGesture } from "./petGesture";

let gesture: ReturnType<typeof createPetGesture> | undefined;
afterEach(() => gesture?.destroy());
function pointer(type: string, options: PointerEventInit = {}) {
  return new PointerEvent(type, { pointerId: 1, isPrimary: true, button: 0, clientX: 10, clientY: 10, cancelable: true, ...options });
}
const click = () => new MouseEvent("click", { detail: 1 });

it("prevents image dragging while retaining a normal pet click", () => {
  const drag = vi.fn(async () => {});
  gesture = createPetGesture(drag, vi.fn());
  const down = pointer("pointerdown");
  gesture.pointerdown(down);
  expect(down.defaultPrevented).toBe(true);
  window.dispatchEvent(pointer("pointermove", { clientX: 12 }));
  window.dispatchEvent(pointer("pointerup"));
  expect(drag).not.toHaveBeenCalled();
  expect(gesture.click(click())).toBe(true);
});

it("hands movement to the native window once and suppresses the resulting click", () => {
  const drag = vi.fn(async () => {});
  gesture = createPetGesture(drag, vi.fn());
  gesture.pointerdown(pointer("pointerdown"));
  window.dispatchEvent(pointer("pointermove", { pointerId: 2, clientX: 40 }));
  expect(drag).not.toHaveBeenCalled();
  const move = pointer("pointermove", { clientX: 13 });
  window.dispatchEvent(move);
  window.dispatchEvent(pointer("pointermove", { clientX: 40 }));
  expect(move.defaultPrevented).toBe(true);
  expect(drag).toHaveBeenCalledTimes(1);
  expect(gesture.click(click())).toBe(false);
  gesture.pointerdown(pointer("pointerdown"));
  expect(gesture.click(click())).toBe(true);
});

it.each(["pointerup", "pointercancel", "destroy"])("cleans up a pending gesture on %s", end => {
  const drag = vi.fn(async () => {});
  gesture = createPetGesture(drag, vi.fn());
  gesture.pointerdown(pointer("pointerdown"));
  if (end === "destroy") gesture.destroy();
  else window.dispatchEvent(pointer(end));
  window.dispatchEvent(pointer("pointermove", { clientX: 40 }));
  expect(drag).not.toHaveBeenCalled();
});

it("ignores other buttons and preserves keyboard activation after a native drag", () => {
  const drag = vi.fn(async () => {});
  gesture = createPetGesture(drag, vi.fn());
  const secondary = pointer("pointerdown", { button: 2 });
  gesture.pointerdown(secondary);
  expect(secondary.defaultPrevented).toBe(false);
  window.dispatchEvent(pointer("pointermove", { clientX: 40 }));
  expect(drag).not.toHaveBeenCalled();
  gesture.pointerdown(pointer("pointerdown"));
  window.dispatchEvent(pointer("pointermove", { clientX: 40 }));
  expect(gesture.click(new MouseEvent("click", { detail: 0 }))).toBe(true);
});

it("reports a native drag failure", async () => {
  const error = new Error("window unavailable");
  const report = vi.fn();
  gesture = createPetGesture(async () => { throw error; }, report);
  gesture.pointerdown(pointer("pointerdown"));
  window.dispatchEvent(pointer("pointermove", { clientX: 40 }));
  await Promise.resolve();
  expect(report).toHaveBeenCalledWith(error);
});
