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
      "codeNew",
      "codeRefresh",
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

describe("mobile Workshop chrome", () => {
  it("keeps refresh beside Activity in the top chrome", () => {
    expect(resolveMobileChromeSurface("more", "list", "runtime")).toBe("runtime");
    expect(mobileChromeLeading("runtime")).toBe("back");
    expect(mobileChromeTrailing("runtime")).toEqual(["runtimeRefresh", "activity"]);
  });
});

describe("mobile Calendar chrome", () => {
  it("uses calendar actions instead of generic Activity chrome", () => {
    expect(resolveMobileChromeSurface("more", "list", "calendar")).toBe("calendar");
    expect(mobileChromeLeading("calendar")).toBe("back");
    expect(mobileChromeTrailing("calendar")).toEqual([
      "calendarView",
      "calendarSearch",
      "calendarNew",
    ]);
  });
});

describe("mobile Scripts chrome", () => {
  it("keeps only document actions in the editor chrome", () => {
    expect(mobileChromeTrailing("automations", "scripts", "script-editor")).toEqual([
      "scriptSave",
      "scriptRun",
      "scriptMore",
    ]);
  });

  it("leaves section switching to the persistent bottom dock", () => {
    expect(mobileChromeTrailing("automations", "scripts", "browse")).toEqual([
      "search",
      "scriptTools",
    ]);
    expect(mobileChromeTrailing("automations", "flows", "browse")).toEqual([
      "search",
      "newAutomation",
    ]);
    expect(mobileChromeTrailing("automations", "history", "browse")).toEqual([
      "search",
    ]);
  });

  it("uses agent actions when Agents is selected inside Automations", () => {
    expect(mobileChromeTrailing("automations", "agents", "browse")).toEqual([
      "search",
      "agentsFilter",
      "agentsImport",
    ]);
  });
});
