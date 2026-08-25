import { describe, expect, it } from "vitest";

import {
  mobileChromeLeading,
  mobileChromeTrailing,
  resolveMobileChromeSurface,
} from "./mobileTopChrome";

describe("mobile menu placement", () => {
  it("keeps the Home menu trigger on the leading edge", () => {
    expect(mobileChromeLeading("home")).toBe("menu");
    expect(mobileChromeTrailing("home")).toEqual([]);
  });
});

describe("mobile Code chrome", () => {
  it("resolves the Code destination to its own surface", () => {
    expect(resolveMobileChromeSurface("more", "list", "code")).toBe("code");
    expect(mobileChromeLeading("code")).toBe("back");
  });

  it("exposes project-mode trailing actions without Activity", () => {
    expect(mobileChromeTrailing("code", "scripts", "browse", "projects")).toEqual([
      "codeSearch",
    ]);
    expect(mobileChromeTrailing("code", "scripts", "browse", "files")).toEqual([
      "codeSearch",
      "codeThread",
    ]);
    expect(mobileChromeTrailing("code", "scripts", "browse", "editor")).toEqual([
      "codeSave",
      "codeFind",
      "codeThread",
    ]);
    expect(mobileChromeTrailing("code", "scripts", "browse", "terminal")).toEqual([
      "codeThread",
    ]);
  });
});
