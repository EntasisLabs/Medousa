/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import { ChatScrollFollower } from "./chatScrollFollower";

let controller: ChatScrollFollower | undefined;
afterEach(() => {
  controller?.destroy();
  controller = undefined;
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

function fixture() {
  const root = document.createElement("div");
  document.body.append(root);
  let height = 1_000;
  let viewport = 300;
  Object.defineProperties(root, {
    scrollHeight: { configurable: true, get: () => height },
    clientHeight: { get: () => viewport },
  });
  root.scrollTop = 700;
  const writes = vi.fn((options: ScrollToOptions) => {
    if (options.behavior !== "smooth") root.scrollTop = options.top ?? root.scrollTop;
  });
  Object.defineProperty(root, "scrollTo", { value: writes });
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  const following = vi.fn();
  const measured = vi.fn();
  controller = new ChatScrollFollower(root, following, measured);
  return {
    root, writes, following, measured, controller,
    grow(value: number) { height = value; controller!.layoutChanged(); },
    resize(value: number) { viewport = value; controller!.layoutChanged(); },
    flush() {
      const pending = [...frames.values()];
      frames.clear();
      pending.forEach((callback) => callback(0));
    },
    scroll(top: number) { root.scrollTop = top; controller!.onScroll(); },
    get pendingFrames() { return frames.size; },
  };
}

describe("chat scroll following", () => {
  it("coalesces layout growth, tools and viewport changes into one bottom write", () => {
    const f = fixture();
    f.grow(1_050);
    f.grow(1_100);
    f.resize(250);
    f.controller.request();
    expect(f.pendingFrames).toBe(1);
    f.flush();
    expect(f.writes).toHaveBeenCalledExactlyOnceWith({ top: 850, behavior: "auto" });
    f.controller.onScroll();
    expect(f.following).toHaveBeenLastCalledWith(true);
    // A status-only update does not write the same scroll position again.
    f.controller.layoutChanged();
    f.flush();
    expect(f.writes).toHaveBeenCalledTimes(1);
  });

  it("upward wheel input cancels even a queued forced follow before a scroll event", () => {
    const f = fixture();
    f.grow(1_200);
    f.controller.request(true);
    f.root.dispatchEvent(new WheelEvent("wheel", { deltaY: -1 }));
    f.flush();
    expect(f.writes).not.toHaveBeenCalled();
    expect(f.following).toHaveBeenLastCalledWith(false);
    f.scroll(690);
    f.grow(1_400);
    f.flush();
    expect(f.root.scrollTop).toBe(690);
    expect(f.writes).not.toHaveBeenCalled();
    // Only actually returning to the bottom resumes follow.
    f.scroll(1_100);
    expect(f.following).toHaveBeenLastCalledWith(true);
    f.grow(1_500);
    f.flush();
    expect(f.root.scrollTop).toBe(1_200);
  });

  it("touch reading and scrollbar movement disengage follow", () => {
    const f = fixture();
    const touch = (type: string, clientY: number) => {
      const event = new Event(type);
      Object.defineProperty(event, "touches", { value: [{ clientY }] });
      f.root.dispatchEvent(event);
    };
    touch("touchstart", 100);
    touch("touchmove", 120);
    f.grow(1_100);
    f.flush();
    expect(f.writes).not.toHaveBeenCalled();
    f.controller.reset();
    f.scroll(650);
    f.grow(1_200);
    f.flush();
    expect(f.following).toHaveBeenLastCalledWith(false);
    expect(f.writes).not.toHaveBeenCalled();
  });

  it("does not mistake content shrinking for a user scrolling upward", () => {
    const f = fixture();
    // Browser clamps the offset when a completed block reduces the extent.
    Object.defineProperty(f.root, "scrollHeight", { configurable: true, get: () => 800 });
    f.scroll(500);
    f.controller.layoutChanged();
    f.flush();
    expect(f.following).toHaveBeenLastCalledWith(true);
    expect(f.writes).not.toHaveBeenCalled();
  });

  it("preserves the visible block when a diagram above a reader settles, without double anchoring", () => {
    const f = fixture();
    const block = document.createElement("div");
    block.dataset.stableMarkdownBlock = "";
    const paragraph = document.createElement("p");
    block.append(paragraph);
    f.root.append(block);
    let paragraphTop = 750;
    f.root.getBoundingClientRect = () => ({ top: 0, bottom: 300 }) as DOMRect;
    paragraph.getBoundingClientRect = () => ({
      top: paragraphTop - f.root.scrollTop, bottom: paragraphTop - f.root.scrollTop + 100,
    }) as DOMRect;
    f.root.dispatchEvent(new WheelEvent("wheel", { deltaY: -1 }));
    f.scroll(600);
    paragraphTop += 80;
    f.grow(1_080);
    f.flush();
    expect(f.root.scrollTop).toBe(680);
    expect(f.following).toHaveBeenLastCalledWith(false);
    // Simulate native anchoring occurring before our layout callback.
    paragraphTop += 40;
    f.root.scrollTop += 40;
    f.grow(1_120);
    f.flush();
    expect(f.root.scrollTop).toBe(720);
    expect(f.writes).not.toHaveBeenCalled();
  });

  it("keeps explicit smooth navigation pinned through intermediate scroll events", () => {
    const f = fixture();
    f.controller.stopFollowing();
    f.scroll(200);
    f.controller.request(true, "smooth");
    f.flush();
    f.scroll(400);
    expect(f.following).toHaveBeenLastCalledWith(true);
    f.grow(1_200);
    f.flush();
    expect(f.writes).toHaveBeenCalledTimes(1);
    f.scroll(700);
    f.flush();
    expect(f.root.scrollTop).toBe(900);
  });

  it("cancels frames on reset/destroy and leaves composer navigation alone", () => {
    const f = fixture();
    const textarea = document.createElement("textarea");
    f.root.append(textarea);
    textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true }));
    expect(f.following).toHaveBeenLastCalledWith(true);
    f.grow(1_100);
    f.controller.reset();
    expect(f.pendingFrames).toBe(1);
    f.flush();
    expect(f.root.scrollTop).toBe(800);
    f.grow(1_200);
    f.controller.destroy();
    expect(f.pendingFrames).toBe(0);
    expect(f.root.style.overflowAnchor).toBe("");
  });

  it("cancels native smooth navigation when switching chats or unmounting", () => {
    const f = fixture();
    f.controller.stopFollowing();
    f.scroll(200);
    f.controller.request(true, "smooth");
    f.flush();
    f.scroll(400);
    f.controller.reset();
    expect(f.writes).toHaveBeenLastCalledWith({ top: 400, behavior: "auto" });
    f.flush();
    expect(f.root.scrollTop).toBe(700);
    f.scroll(200);
    f.controller.request(true, "smooth");
    f.flush();
    f.controller.destroy();
    expect(f.writes).toHaveBeenLastCalledWith({ top: 200, behavior: "auto" });
    expect(f.pendingFrames).toBe(0);
  });
});
