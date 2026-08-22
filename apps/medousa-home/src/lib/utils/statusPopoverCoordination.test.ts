import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  announceStatusPopoverOpen,
  closeOnOtherStatusPopover,
} from "./statusPopoverCoordination";

describe("statusPopoverCoordination", () => {
  beforeEach(() => {
    vi.stubGlobal("window", new EventTarget());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("closes an open status popover when a different one opens", () => {
    const close = vi.fn();
    const stop = closeOnOtherStatusPopover("workshops", close);

    announceStatusPopoverOpen("layout");

    expect(close).toHaveBeenCalledOnce();
    stop();
  });

  it("does not close the popover that announced itself", () => {
    const close = vi.fn();
    const stop = closeOnOtherStatusPopover("activity", close);

    announceStatusPopoverOpen("activity");

    expect(close).not.toHaveBeenCalled();
    stop();
  });
});
