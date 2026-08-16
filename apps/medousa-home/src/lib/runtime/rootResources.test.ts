import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import {
  APP_SHELL_EAGER_MODULES,
  APP_SHELL_ROOT_RESOURCE_IDS,
  SHELL_A11Y_FIXTURES,
  bindRootResource,
  listLiveRootResources,
  recordRootResource,
  resetRootResourcesForTests,
} from "./rootResources";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

afterEach(() => {
  resetRootResourcesForTests();
});

describe("root resource probe", () => {
  it("lists AppShell-started pollers and listeners", () => {
    expect([...APP_SHELL_ROOT_RESOURCE_IDS]).toEqual([
      "wizard-bootstrap",
      "viewport-tracking",
      "native-mobile-layout",
      "mobile-viewport",
      "mobile-native",
      "peer-message-notifications",
      "agent-browser-coord",
      "command-spotlight-hotkeys",
    ]);
    const source = readFileSync(
      join(homeRoot, "src/lib/components/layout/AppShell.svelte"),
      "utf8",
    );
    for (const id of APP_SHELL_ROOT_RESOURCE_IDS) {
      expect(source, `AppShell must record ${id}`).toContain(`"${id}"`);
    }
  });

  it("releases recorded resources on dispose", () => {
    const stop = bindRootResource("peer-message-notifications", () => {});
    expect(listLiveRootResources()).toEqual(["peer-message-notifications"]);
    stop();
    expect(listLiveRootResources()).toEqual([]);
  });

  it("detects a leftover listener after a missed dispose", () => {
    recordRootResource("agent-browser-coord");
    expect(listLiveRootResources()).toContain("agent-browser-coord");
  });
});

describe("eager AppShell graph freeze", () => {
  it("still statically imports dormant feature overlays", () => {
    const source = readFileSync(
      join(homeRoot, "src/lib/components/layout/AppShell.svelte"),
      "utf8",
    );
    for (const name of APP_SHELL_EAGER_MODULES) {
      expect(source, `missing eager import ${name}`).toContain(name);
    }
    expect(source).not.toMatch(/import WorkshopShell from/);
    expect(source).not.toMatch(/import MobileShell from/);
    expect(source).toContain("probeClientPlatform");
    expect(source).toContain("loadDesktopShell");
    expect(source).toContain("loadMobileShell");
  });
});

describe("shell and chat a11y fixtures", () => {
  it("keeps desktop/mobile/chat landmarks used by later composition splits", () => {
    for (const fixture of Object.values(SHELL_A11Y_FIXTURES)) {
      const source = readFileSync(join(homeRoot, fixture.file), "utf8");
      for (const needle of fixture.mustContain) {
        expect(source, `${fixture.file} missing ${needle}`).toContain(needle);
      }
    }
  });
});
