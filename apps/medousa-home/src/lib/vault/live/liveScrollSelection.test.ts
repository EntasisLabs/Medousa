/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
import { handleLiveScrollToSelection } from "./liveScrollSelection";
import type { EditorView } from "@tiptap/pm/view";

describe("handleLiveScrollToSelection", () => {
  it("suppresses default scroll without touching scrollTop", () => {
    const host = document.createElement("div");
    host.className = "vault-live-editor";
    Object.defineProperty(host, "scrollTop", {
      configurable: true,
      writable: true,
      value: 140,
    });
    const prose = document.createElement("div");
    host.append(prose);
    document.body.append(host);

    const view = {
      dom: prose,
      state: { selection: { from: 1, to: 1, empty: true } },
      coordsAtPos: () => ({ top: 900, bottom: 920, left: 0, right: 1 }),
    } as unknown as EditorView;

    vi.spyOn(host, "getBoundingClientRect").mockReturnValue({
      top: 0,
      bottom: 400,
      left: 0,
      right: 400,
      width: 400,
      height: 400,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    expect(handleLiveScrollToSelection(view)).toBe(true);
    expect(host.scrollTop).toBe(140);
    host.remove();
  });
});
