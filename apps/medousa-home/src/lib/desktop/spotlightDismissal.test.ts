import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { spotlightDismissal, type SpotlightPointer } from "./spotlightDismissal";

beforeEach(() => vi.useFakeTimers());
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
function setup(initial: SpotlightPointer | null = null) {
  const pointer = vi.fn<() => Promise<SpotlightPointer | null>>().mockResolvedValue(initial);
  const dismiss = vi.fn();
  const releaseInside = vi.fn();
  const canDismiss = vi.fn(() => true);
  const policy = spotlightDismissal({ pointer, canDismiss, dismiss, releaseInside });
  return { policy, pointer, dismiss, canDismiss, releaseInside };
}
const held = { buttonsDown: true, inside: false };
const outside = { buttonsDown: false, inside: false };
const inside = { buttonsDown: false, inside: true };

it("keeps a slow outside press visible until release, then dismisses a plain click", async () => {
  const { policy, pointer, dismiss } = setup(held);
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(5000);
  expect(dismiss).not.toHaveBeenCalled();
  pointer.mockResolvedValue(outside);
  await vi.advanceTimersByTimeAsync(32);
  expect(dismiss).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});
it("keeps a drag released inside visible without requesting focus while dragging", async () => {
  const { policy, pointer, dismiss, releaseInside } = setup(held);
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(64);
  expect(releaseInside).not.toHaveBeenCalled();
  pointer.mockResolvedValue(inside);
  await vi.advanceTimersByTimeAsync(32);
  expect(dismiss).not.toHaveBeenCalled();
  expect(releaseInside).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});
it("dismisses a cancelled drag released outside", async () => {
  const { policy, pointer, dismiss } = setup(held);
  policy.focusChanged(false);
  policy.dragChanged("enter"); policy.dragChanged("leave");
  await vi.advanceTimersByTimeAsync(32);
  pointer.mockResolvedValue(outside);
  await vi.advanceTimersByTimeAsync(32);
  expect(dismiss).toHaveBeenCalledOnce();
});
it("ordinary keyboard focus loss dismisses even when the cursor happens to be inside", async () => {
  const { policy, dismiss } = setup(inside);
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(0);
  expect(dismiss).toHaveBeenCalledOnce();
});
it.each(["refocus", "drop", "dismiss"])("ignores a stale native reply after %s", async action => {
  const { policy, pointer, dismiss } = setup();
  let resolve!: (pointer: SpotlightPointer) => void;
  pointer.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  policy.focusChanged(false);
  if (action === "refocus") policy.focusChanged(true);
  else if (action === "drop") policy.dragChanged("drop");
  else policy.cancel();
  resolve(outside);
  await vi.advanceTimersByTimeAsync(100);
  expect(dismiss).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});
it("handles a fast drag that arrives before the first native reply", async () => {
  const { policy, dismiss } = setup(inside);
  policy.dragChanged("enter"); policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(0);
  expect(dismiss).not.toHaveBeenCalled();
});
it("keeps in-flight shell actions protected across focus loss", async () => {
  const { policy, pointer, canDismiss, dismiss } = setup(outside);
  canDismiss.mockReturnValue(false);
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(100);
  expect(pointer).not.toHaveBeenCalled();
  expect(dismiss).not.toHaveBeenCalled();
});
it("falls back to ordinary dismissal on unsupported platforms or probe failure", async () => {
  const { policy, pointer, dismiss } = setup();
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(0);
  expect(dismiss).toHaveBeenCalledOnce();
  pointer.mockRejectedValueOnce(new Error("Window closed"));
  policy.focusChanged(false);
  await vi.advanceTimersByTimeAsync(0);
  expect(dismiss).toHaveBeenCalledTimes(2);
});

it("only requests native focus after a completed drop, including a drop over the header", () => {
  const { policy, releaseInside } = setup(held);
  policy.dragChanged("enter"); policy.dragChanged("over");
  expect(releaseInside).not.toHaveBeenCalled();
  policy.dragChanged("drop");
  expect(releaseInside).toHaveBeenCalledOnce();
});
it("does not refocus a dismissed launcher for a late drop event", () => {
  const { policy, releaseInside, canDismiss } = setup();
  canDismiss.mockReturnValue(false);
  policy.dragChanged("drop");
  expect(releaseInside).not.toHaveBeenCalled();
});
