// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from "vitest";
import { attachComposerMenuDismiss } from "./composerMenuDismiss";

afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

describe("attachComposerMenuDismiss", () => {
  it("does not dismiss when an inside action removes its clicked element", () => {
    vi.useFakeTimers();
    const panel = document.createElement("div");
    const action = document.createElement("button");
    panel.append(action);
    document.body.append(panel);

    const onDismiss = vi.fn();
    const detach = attachComposerMenuDismiss({
      isInside: (target) => target !== null && panel.contains(target),
      onDismiss,
    });
    vi.runAllTimers();

    action.addEventListener("click", () => action.remove());
    action.click();

    expect(onDismiss).not.toHaveBeenCalled();
    detach();
  });

  it("dismisses for outside clicks and Escape", () => {
    vi.useFakeTimers();
    const panel = document.createElement("div");
    const outside = document.createElement("button");
    document.body.append(panel, outside);

    const onDismiss = vi.fn();
    const detach = attachComposerMenuDismiss({
      isInside: (target) => target !== null && panel.contains(target),
      onDismiss,
    });
    vi.runAllTimers();

    outside.click();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

    expect(onDismiss).toHaveBeenCalledTimes(2);
    detach();
  });
});
