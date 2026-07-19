import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import { setDesktopShellChrome } from "$lib/utils/environmentCanvasOps";
import {
  preferredModeDesktopChromeSeed,
  resolveDesktopShellChrome,
  seedDesktopShellChromeFromPreferredMode,
} from "./desktopEnvironmentChrome";

describe("desktopEnvironmentChrome", () => {
  it("resolves defaults when desktop chrome is unset", () => {
    const resolved = resolveDesktopShellChrome(defaultEnvironmentSpec());
    expect(resolved).toEqual({
      navStyle: "rail",
      activityRail: "visible",
      vaultChatFab: true,
      vaultSidebar: "visible",
    });
  });

  it("resolves explicit desktop chrome fields", () => {
    const spec = defaultEnvironmentSpec();
    spec.shellChrome = {
      desktop: {
        activityRail: "hidden",
        vaultChatFab: false,
        vaultSidebar: "hidden",
        navStyle: "compact",
      },
    };
    expect(resolveDesktopShellChrome(spec)).toEqual({
      navStyle: "compact",
      activityRail: "hidden",
      vaultChatFab: false,
      vaultSidebar: "hidden",
    });
  });

  it("preferred-mode seed maps workspace vs workspace-ai", () => {
    expect(preferredModeDesktopChromeSeed("workspace")).toEqual({
      vaultChatFab: false,
      activityRail: "collapsed",
    });
    expect(preferredModeDesktopChromeSeed("workspace-ai")).toEqual({
      vaultChatFab: true,
      activityRail: "visible",
    });
  });

  it("seeds only unset desktop chrome fields", () => {
    const spec = defaultEnvironmentSpec();
    expect(seedDesktopShellChromeFromPreferredMode(spec, "workspace")).toBe(true);
    expect(spec.shellChrome?.desktop).toMatchObject({
      vaultChatFab: false,
      activityRail: "collapsed",
    });

    // User edited FAB on — seed again must not clobber.
    setDesktopShellChrome(spec, { vaultChatFab: true });
    expect(seedDesktopShellChromeFromPreferredMode(spec, "workspace")).toBe(false);
    expect(spec.shellChrome?.desktop?.vaultChatFab).toBe(true);
    expect(spec.shellChrome?.desktop?.activityRail).toBe("collapsed");
  });

  it("setDesktopShellChrome patches spec and active preset", () => {
    const spec = defaultEnvironmentSpec();
    setDesktopShellChrome(spec, {
      vaultChatFab: false,
      activityRail: "hidden",
      vaultSidebar: "hidden",
    });
    expect(spec.shellChrome?.desktop).toMatchObject({
      vaultChatFab: false,
      activityRail: "hidden",
      vaultSidebar: "hidden",
    });
    const active = spec.layoutPresets?.find((preset) => preset.active);
    expect(active?.shellChrome?.desktop).toMatchObject({
      vaultChatFab: false,
      activityRail: "hidden",
      vaultSidebar: "hidden",
    });
  });
});
