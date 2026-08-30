/** @vitest-environment happy-dom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dismissBootstrapSplash } from "./bootstrapSplash";

describe("bootstrap splash handoff", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="medousa-bootstrap-splash"></div>';
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      callback(0);
      return 1;
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    document.body.innerHTML = "";
  });

  it("fades and removes the static splash after a mounted view signals readiness", () => {
    dismissBootstrapSplash();

    const splash = document.getElementById("medousa-bootstrap-splash");
    expect(splash?.getAttribute("data-exiting")).toBe("true");

    vi.advanceTimersByTime(300);
    expect(document.getElementById("medousa-bootstrap-splash")).toBeNull();
  });

  it("schedules the handoff only once", () => {
    dismissBootstrapSplash();
    dismissBootstrapSplash();

    expect(window.requestAnimationFrame).toHaveBeenCalledTimes(1);
  });
});
