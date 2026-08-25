import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import {
  shellAskFabVisible,
  showBuiltinHomeInlineAsk,
  visibleMobileTabs,
} from "$lib/utils/mobileEnvironmentChrome";

describe("mobileEnvironmentChrome", () => {
  it("shows all mobile navigation doors by default", () => {
    expect(visibleMobileTabs(defaultEnvironmentSpec())).toEqual([
      "home",
      "chat",
      "notes",
      "web",
      "more",
    ]);
  });

  it("ignores retired tab-bar density and uses shared layout membership", () => {
    const spec = defaultEnvironmentSpec();
    spec.shellChrome = {
      mobile: {
        defaultHome: "home",
        askEntry: "inline",
        tabBar: "minimal",
      },
    };
    spec.layoutPresets![0]!.surfaces = spec.layoutPresets![0]!.surfaces.filter(
      (id) => id !== "notes" && id !== "web",
    );
    expect(visibleMobileTabs(spec)).toEqual([
      "home",
      "chat",
      "more",
    ]);
  });

  it("shell FAB for fab ask on builtin home", () => {
    expect(
      shellAskFabVisible({
        askEntry: "fab",
        customHome: false,
        fabChromeActionCount: 0,
      }),
    ).toBe(true);
    expect(showBuiltinHomeInlineAsk("fab")).toBe(false);
  });

  it("defers to chrome_action fab on custom home when present", () => {
    expect(
      shellAskFabVisible({
        askEntry: "fab",
        customHome: true,
        fabChromeActionCount: 1,
      }),
    ).toBe(false);
  });
});
