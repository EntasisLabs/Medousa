/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
import { handleLiveScrollToSelection } from "./liveScrollSelection";
import type { EditorView } from "@tiptap/pm/view";

function mockView(opts: {
  hostScrollTop: number;
  caretTop: number;
  caretBottom: number;
  hostTop: number;
  hostBottom: number;
}): { view: EditorView; host: HTMLElement } {
  const host = document.createElement("div");
  host.className = "vault-live-editor";
  Object.defineProperty(host, "scrollTop", {
    configurable: true,
    writable: true,
    value: opts.hostScrollTop,
  });
  vi.spyOn(host, "getBoundingClientRect").mockReturnValue({
    top: opts.hostTop,
    bottom: opts.hostBottom,
    left: 0,
    right: 400,
    width: 400,
    height: opts.hostBottom - opts.hostTop,
    x: 0,
    y: opts.hostTop,
    toJSON: () => ({}),
  });

  const prose = document.createElement("div");
  prose.className = "ProseMirror";
  host.append(prose);
  document.body.append(host);

  const view = {
    dom: prose,
    state: {
      selection: { from: 1, to: 1, empty: true },
    },
    coordsAtPos: () => ({
      top: opts.caretTop,
      bottom: opts.caretBottom,
      left: 10,
      right: 12,
    }),
  } as unknown as EditorView;

  return { view, host };
}

describe("handleLiveScrollToSelection", () => {
  it("does not scroll when caret is already visible", () => {
    const { view, host } = mockView({
      hostScrollTop: 100,
      hostTop: 0,
      hostBottom: 400,
      caretTop: 120,
      caretBottom: 140,
    });
    expect(handleLiveScrollToSelection(view)).toBe(true);
    expect(host.scrollTop).toBe(100);
    host.remove();
  });

  it("scrolls the Live host when caret is below the viewport", () => {
    const { view, host } = mockView({
      hostScrollTop: 0,
      hostTop: 0,
      hostBottom: 400,
      caretTop: 420,
      caretBottom: 440,
    });
    expect(handleLiveScrollToSelection(view)).toBe(true);
    // margin 28 → delta = 440 - (400 - 28) = 68
    expect(host.scrollTop).toBe(68);
    host.remove();
  });
});
