/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { SplitNode } from "$lib/types/shellTabs";

const state = vi.hoisted(() => ({
  tabs: [{ id: "tab-a", title: "README.md" }],
  splitRoot: { type: "group", id: "source" } as SplitNode,
  tabDropTargetGroupId: null as string | null,
  tabDropSplitEdge: null as { groupId: string; edge: string } | null,
  activate: vi.fn(), splitGroupWithTab: vi.fn(() => true), moveTab: vi.fn(), focusGroup: vi.fn(),
}));
const overlays = vi.hoisted(() => ({ pushBrowserPopoverOverlay: vi.fn(), popBrowserPopoverOverlay: vi.fn() }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: state }));
vi.mock("$lib/utils/browserPopoverOverlay", () => overlays);
import { beginShellTabDrag } from "./shellTabDrag";

let trigger: HTMLDivElement;
let host: HTMLDivElement;
let hit: Element | null;
function pointer(type: string, x: number, y: number, pointerId = 1) {
  return new PointerEvent(type, { pointerId, button: 0, clientX: x, clientY: y, bubbles: true, cancelable: true });
}
function start() {
  trigger.addEventListener("pointerdown", event => beginShellTabDrag(event, "tab-a", "source"), { once: true });
  const down = pointer("pointerdown", 100, 20);
  trigger.dispatchEvent(down);
  return down;
}
function move(x: number, y: number, id = 1) { document.dispatchEvent(pointer("pointermove", x, y, id)); }
function release(x: number, y: number, id = 1) { document.dispatchEvent(pointer("pointerup", x, y, id)); }
beforeEach(() => {
  vi.clearAllMocks();
  state.splitRoot = { type: "group", id: "source" };
  state.tabDropTargetGroupId = null; state.tabDropSplitEdge = null;
  trigger = document.createElement("div");
  Object.assign(trigger, { setPointerCapture: vi.fn(), hasPointerCapture: () => false });
  host = document.createElement("div"); host.className = "shell-pane"; host.dataset.groupId = "source";
  vi.spyOn(host, "getBoundingClientRect").mockReturnValue({ left: 0, top: 40, right: 1000, bottom: 640, width: 1000, height: 600 } as DOMRect);
  document.body.append(trigger, host); hit = host;
  vi.spyOn(document, "elementFromPoint").mockImplementation(() => hit);
});
afterEach(() => {
  window.dispatchEvent(new Event("blur"));
  document.body.replaceChildren(); vi.restoreAllMocks();
});
it("keeps clicks and small pointer jitter as tab activation", () => {
  expect(start().defaultPrevented).toBe(true);
  move(103, 22); release(103, 22);
  expect(state.activate).toHaveBeenCalledWith("tab-a");
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
  expect(overlays.pushBrowserPopoverOverlay).not.toHaveBeenCalled();
});
it.each([[150, 340, "left"], [850, 340, "right"], [500, 140, "top"], [500, 540, "bottom"]] as const)("previews and splits at a generous editor target (%s, %s)", (x, y, edge) => {
  start(); move(x, y);
  expect(state.tabDropSplitEdge).toEqual({ groupId: "source", edge });
  expect(document.querySelector(".shell-tab-drag-ghost")?.textContent).toBe("README.md");
  release(x, y);
  expect(state.splitGroupWithTab).toHaveBeenCalledWith("source", "tab-a", edge);
  expect(state.activate).not.toHaveBeenCalled();
  expect(state.tabDropSplitEdge).toBeNull();
  expect(document.querySelector(".shell-tab-drag-ghost")).toBeNull();
  expect(overlays.pushBrowserPopoverOverlay).toHaveBeenCalledOnce();
  expect(overlays.popBrowserPopoverOverlay).toHaveBeenCalledOnce();
});
it("moves to the center of another pane using the shared tab model", () => {
  host.dataset.groupId = "other";
  start(); move(500, 340);
  expect(state.tabDropTargetGroupId).toBe("other");
  release(500, 340);
  expect(state.moveTab).toHaveBeenCalledWith("tab-a", "other");
  expect(state.focusGroup).toHaveBeenCalledWith("other");
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
});
it("retains mini-controller drop behavior and ignores identity-only tab strips", () => {
  host.className = "shell-tab-notch-pane";
  host.dataset.groupId = "other";
  start(); move(150, 340);
  expect(state.tabDropTargetGroupId).toBe("other");
  host.className = "shell-tab-strip";
  move(980, 340); release(980, 340);
  expect(state.moveTab).not.toHaveBeenCalled();
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
  expect(state.activate).not.toHaveBeenCalled();
});
it.each(["escape", "cancel", "blur", "capture"])("cleans up %s without moving or activating a tab", reason => {
  start(); move(850, 340);
  if (reason === "escape") document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  else if (reason === "blur") window.dispatchEvent(new Event("blur"));
  else if (reason === "capture") trigger.dispatchEvent(pointer("lostpointercapture", 850, 340));
  else document.dispatchEvent(pointer("pointercancel", 850, 340));
  release(850, 340);
  expect(state.tabDropSplitEdge).toBeNull();
  expect(document.body.classList.contains("shell-tab-dragging")).toBe(false);
  expect(document.querySelector(".shell-tab-drag-ghost")).toBeNull();
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
  expect(state.activate).not.toHaveBeenCalled();
  expect(overlays.popBrowserPopoverOverlay).toHaveBeenCalledOnce();
  start(); release(100, 20);
  expect(state.activate).toHaveBeenCalledOnce();
});
it("ignores another pointer and does not activate when dropped outside all panes", () => {
  start(); move(850, 340); release(850, 340, 2);
  expect(state.tabDropSplitEdge).not.toBeNull();
  hit = null; release(500, 340);
  expect(state.activate).not.toHaveBeenCalled();
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
  expect(state.tabDropSplitEdge).toBeNull();
});
it("respects the four-pane limit while allowing a move to an existing pane", () => {
  state.splitRoot = { type: "branch", id: "root", direction: "column", ratio: .5,
    a: { type: "branch", id: "a", direction: "row", ratio: .5, a: { type: "group", id: "source" }, b: { type: "group", id: "b" } },
    b: { type: "branch", id: "b", direction: "row", ratio: .5, a: { type: "group", id: "other" }, b: { type: "group", id: "d" } } };
  host.dataset.groupId = "other";
  start(); move(850, 340); release(850, 340);
  expect(state.splitGroupWithTab).not.toHaveBeenCalled();
  expect(state.moveTab).toHaveBeenCalledWith("tab-a", "other");
});
