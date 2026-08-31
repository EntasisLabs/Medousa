import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import {
  APP_SHELL_EAGER_MODULES,
  APP_SHELL_LAZY_OVERLAYS,
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
      "command-spotlight-hotkeys",
      "work-ask-focus",
    ]);
    const source = readFileSync(
      join(homeRoot, "src/lib/runtime/shellLifecycle.ts"),
      "utf8",
    );
    for (const id of APP_SHELL_ROOT_RESOURCE_IDS) {
      expect(source, `shell lifecycle must record ${id}`).toContain(`"${id}"`);
    }
    expect(source).toContain("wizard.bootstrap(wizardBootstrap.signal)");
    expect(source).not.toContain("attachAgentBrowserCoord");
  });

  it("releases recorded resources on dispose", () => {
    const stop = bindRootResource("peer-message-notifications", () => {});
    expect(listLiveRootResources()).toEqual(["peer-message-notifications"]);
    stop();
    expect(listLiveRootResources()).toEqual([]);
  });

  it("detects a leftover listener after a missed dispose", () => {
    recordRootResource("peer-message-notifications");
    expect(listLiveRootResources()).toContain("peer-message-notifications");
  });

  it("re-exports destination feature ids for leak probes", async () => {
    const { DESTINATION_FEATURE_IDS } = await import("./rootResources");
    expect(DESTINATION_FEATURE_IDS).toContain("vault-browse");
    expect(DESTINATION_FEATURE_IDS).toContain("code-work");
  });
});

describe("eager AppShell graph freeze", () => {
  it("keeps chat eager and loads overlays only on intent", () => {
    const source = readFileSync(
      join(homeRoot, "src/lib/components/layout/AppShell.svelte"),
      "utf8",
    );
    for (const name of APP_SHELL_EAGER_MODULES) {
      expect(source, `missing eager import ${name}`).toContain(name);
    }
    for (const name of APP_SHELL_LAZY_OVERLAYS) {
      expect(source, `overlay ${name} must not be a static import`).not.toMatch(
        new RegExp(`import ${name} from`),
      );
    }
    expect(source).not.toMatch(/import WorkshopShell from/);
    expect(source).not.toMatch(/import MobileShell from/);
    expect(source).not.toMatch(/from ["']\$lib\/stores\/vault["']/);
    expect(source).not.toMatch(/from ["']\$lib\/stores\/lmeWorkspace/);
    expect(source).not.toMatch(/from ["']\$lib\/stores\/workspace/);
    expect(source).toContain("startShellRootResources");
    expect(source).toContain("ShellChunkError");
    expect(source).toContain("BootstrapSplashHandoff");
    expect(source).not.toContain("HomeSplash");
    expect(source).not.toMatch(/void import\(/);
  });

  it("keeps the bundle-budget overlay denylist in lockstep", () => {
    const budget = readFileSync(
      join(homeRoot, "scripts/verify-bundle-budget.mjs"),
      "utf8",
    );
    for (const name of APP_SHELL_LAZY_OVERLAYS) {
      expect(budget, `budget denylist missing ${name}`).toContain(`"${name}"`);
    }
  });

  it("starts browser coordination inside the browser feature", () => {
    const source = readFileSync(join(homeRoot, "src/lib/runtime/viewLoaders.ts"), "utf8");
    expect(source).toContain('id !== "browser"');
    expect(source).toContain('import("$lib/utils/agentBrowserCoord")');
  });
});

describe("app viewport contract", () => {
  it("keeps a boxed full-height mount for native WebViews", () => {
    const source = readFileSync(join(homeRoot, "src/app.html"), "utf8");
    const mount = source.match(/<div\s+id="medousa-app-root"[\s\S]*?>/)?.[0];

    expect(mount).toBeDefined();
    expect(mount).toContain("height: 100%");
    expect(mount).toContain("width: 100%");
    expect(mount).not.toContain("display: contents");
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
