/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { homeChannelSurface, isBrowserWorkshop, shouldUseMobileShell } from "$lib/platform";
import {
  isCoLocatedWorkshop,
  usesOriginPrivateDaemonStorage,
  vaultHostSideHint,
} from "$lib/utils/workshopLocality";

describe("browser workshop host", () => {
  it("uses the mobile shell and origin-private storage without host folder pickers", () => {
    expect(isBrowserWorkshop()).toBe(true);
    expect(shouldUseMobileShell()).toBe(true);
    expect(homeChannelSurface()).toBe("home-browser");
    expect(isCoLocatedWorkshop()).toBe(false);
    expect(usesOriginPrivateDaemonStorage()).toBe(true);
    expect(vaultHostSideHint()).toContain("browser");
  });
});
