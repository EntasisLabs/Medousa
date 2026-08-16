import { afterEach, describe, expect, it } from "vitest";
import { DESTINATION_FEATURE_IDS, disposeDestinationFeatures } from "./disposeDestinations";
import {
  disposeFeature,
  listLiveFeatureIds,
  loadFeature,
  resetFeaturesForTests,
} from "./loader";
import type { FeatureModule } from "./types";

afterEach(() => {
  resetFeaturesForTests();
});

function stubModule(): FeatureModule {
  return {
    async start() {
      return { dispose() {} };
    },
  };
}

describe("feature start/dispose leak probe", () => {
  it("lists destination features separately from shell platforms", () => {
    expect(DESTINATION_FEATURE_IDS).toContain("vault-browse");
    expect(DESTINATION_FEATURE_IDS).toContain("vault-edit");
    expect(DESTINATION_FEATURE_IDS).toContain("code-work");
    expect(DESTINATION_FEATURE_IDS).toContain("export-import");
    expect(DESTINATION_FEATURE_IDS).not.toContain("shell-desktop");
    expect(DESTINATION_FEATURE_IDS).not.toContain("shell-mobile");
  });

  it.each(["workshop-switch", "platform-switch", "navigate-away"] as const)(
    "empties the live set after %s dispose",
    async (reason) => {
      await loadFeature("vault-browse", async () => stubModule(), {
        platform: "desktop",
      });
      await loadFeature("code-work", async () => stubModule(), {
        platform: "desktop",
      });
      expect(listLiveFeatureIds()).toEqual(["code-work", "vault-browse"]);
      await disposeFeature("vault-browse", reason);
      await disposeFeature("code-work", reason);
      expect(listLiveFeatureIds()).toEqual([]);
    },
  );

  it("disposeDestinationFeatures clears every destination instance", async () => {
    await loadFeature("vault-edit", async () => stubModule(), { platform: "desktop" });
    await loadFeature("export-import", async () => stubModule(), {
      platform: "desktop",
    });
    await loadFeature("browser", async () => stubModule(), { platform: "mobile" });
    expect(listLiveFeatureIds().length).toBe(3);
    await disposeDestinationFeatures("workshop-switch");
    expect(listLiveFeatureIds()).toEqual([]);
  });
});
