import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function source(rel: string): string {
  return readFileSync(join(homeRoot, rel), "utf8");
}

describe("css inventory and cascade", () => {
  it("declares cascade layers centrally without importing feature sheets", () => {
    const app = source("src/app.postcss");
    expect(app).toMatch(/@layer base, components, utilities, features;/);
    expect(app).not.toMatch(/browser\.postcss/);
    expect(app).not.toMatch(/peers\.postcss/);
    expect(app).not.toMatch(/mobile-home-convergence\.postcss/);
    expect(app).toContain("prefers-reduced-motion");
  });

  it("loads browser and peers CSS from their feature entries", () => {
    expect(source("src/lib/components/browser/HumanBrowserPanel.svelte")).toContain(
      "$lib/styles/browser.postcss",
    );
    expect(source("src/lib/components/peers/PeersPanel.svelte")).toContain(
      "$lib/styles/peers.postcss",
    );
    expect(source("src/lib/styles/browser.postcss")).toContain("@layer features");
    expect(source("src/lib/styles/peers.postcss")).toContain("@layer features");
  });

  it("keeps the checked-in inventory honest about pending extracts", () => {
    const inventory = JSON.parse(source("security/css-inventory.json"));
    expect(inventory.layers).toEqual(["base", "components", "utilities", "features"]);
    const pending = inventory.entries.filter((entry: { status?: string }) => entry.status === "pending-extract");
    expect(pending.length).toBeGreaterThan(0);
    const browser = inventory.entries.find((entry: { id: string }) => entry.id === "browser");
    expect(browser.loadedBy).toContain("HumanBrowserPanel");
  });
});

describe("selected-theme Tailwind compile", () => {
  it("does not compile any palette into the Skeleton plugin", () => {
    const config = source("tailwind.config.ts");
    expect(config).not.toMatch(/allThemes/);
    expect(config).not.toMatch(/STARTUP_THEMES/);
    expect(config).toMatch(/custom:\s*\[\s*\]/);
  });
});
