import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function source(rel: string): string {
  return readFileSync(join(homeRoot, rel), "utf8");
}

describe("former small SCC families stay acyclic", () => {
  it("identity does not import userProfiles", () => {
    expect(source("src/lib/stores/identity.svelte.ts")).not.toMatch(/userProfiles/);
  });

  it("voice presets do not import workshop defaults", () => {
    expect(source("src/lib/stores/voicePresets.svelte.ts")).not.toMatch(/workshopDefaults/);
  });

  it("browser popover overlay does not import the compositor", () => {
    expect(source("src/lib/utils/browserPopoverOverlay.ts")).not.toMatch(/browserCompositor/);
  });

  it("custom vault spaces do not import templates", () => {
    expect(source("src/lib/utils/vaultCustomSpaces.ts")).not.toMatch(/vaultTemplates/);
  });

  it("human browser API does not import the UI store", () => {
    const api = source("src/lib/humanBrowser.ts");
    expect(api).not.toMatch(/humanBrowserSurface/);
    expect(api).not.toMatch(/humanBrowser\.svelte/);
  });

  it("undertakings do not import shellTabs", () => {
    expect(source("src/lib/stores/undertakings.svelte.ts")).not.toMatch(/shellTabs/);
  });
});
