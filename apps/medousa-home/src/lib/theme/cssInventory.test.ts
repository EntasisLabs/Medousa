import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function source(rel: string): string {
  return readFileSync(join(homeRoot, rel), "utf8");
}

function findComponent(name: string): string {
  const roots = [
    join(homeRoot, "src/lib/components"),
    join(homeRoot, "src/lib/liquid"),
  ];
  const stack = [...roots];
  while (stack.length) {
    const dir = stack.pop()!;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        stack.push(path);
        continue;
      }
      if (entry.name === name) return path;
    }
  }
  throw new Error(`component ${name} not found`);
}

describe("css inventory and cascade", () => {
  it("declares cascade layers centrally without importing feature sheets", () => {
    const app = source("src/app.postcss");
    expect(app).toMatch(/@layer base, components, utilities, features;/);
    expect(app).toContain("prefers-reduced-motion");
    const inventory = JSON.parse(source("security/css-inventory.json"));
    for (const entry of inventory.entries) {
      if (entry.class !== "feature") continue;
      const filename = String(entry.path).split("/").pop();
      expect(app, `${filename} must not be @imported from app.postcss`).not.toMatch(
        new RegExp(`@import[^;]*${filename?.replace(".", "\\.")}`),
      );
    }
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

  it("loads extracted feature sheets from destination entries", () => {
    const inventory = JSON.parse(source("security/css-inventory.json"));
    expect(inventory.layers).toEqual(["base", "components", "utilities", "features"]);
    const pending = inventory.entries.filter((entry: { status?: string }) => entry.status === "pending-extract");
    expect(pending).toEqual([]);
    const features = inventory.entries.filter((entry: { class?: string }) => entry.class === "feature");
    expect(features.length).toBeGreaterThan(0);
    for (const entry of features) {
      expect(entry.loadedBy, `${entry.id} missing loadedBy`).toMatch(/\S/);
      const sheet = source(entry.path);
      if (entry.layered === false) {
        expect(sheet, `${entry.path} must stay unlayered`).not.toContain("@layer");
      } else {
        expect(sheet, `${entry.path} must wrap in @layer features`).toContain("@layer features");
      }
      const loaders = String(entry.loadedBy)
        .split("/")
        .map((part: string) => part.trim())
        .filter((part: string) => part.endsWith(".svelte"));
      expect(loaders.length, `${entry.id} loadedBy has no component`).toBeGreaterThan(0);
      const importNeedle = `$lib/styles/${String(entry.path).split("/").pop()}`;
      const hits = loaders.filter((name: string) => {
        const text = readFileSync(findComponent(name), "utf8");
        return text.includes(importNeedle);
      });
      expect(hits, `${entry.id} not imported by ${entry.loadedBy}`).not.toEqual([]);
    }
  });

  it("keeps input-chrome unlayered so it beats Tailwind Forms", () => {
    const sheet = source("src/lib/styles/input-chrome.postcss").replace(
      /\/\*[\s\S]*?\*\//g,
      "",
    );
    expect(sheet).not.toContain("@layer");
    expect(source("src/app.postcss")).toMatch(/@import[^;]*input-chrome\.postcss/);
    expect(source("src/app.postcss")).toMatch(/@import[^;]*workshop-primitives\.postcss/);
  });

  it("loads LME rail and mobile library CSS from always-mounted hosts", () => {
    const side = source("src/lib/components/lme/LmeSidePanel.svelte");
    expect(side).toContain("$lib/styles/lme.postcss");
    expect(side).toContain("$lib/styles/vault-browse.postcss");
    expect(side).toContain("$lib/styles/vault-workshop.postcss");
    expect(source("src/lib/components/mobile/MobileCodePanel.svelte")).toContain(
      "$lib/styles/lme.postcss",
    );
    expect(source("src/lib/components/mobile/MobileLibraryPanel.svelte")).toContain(
      "$lib/styles/vault-browse.postcss",
    );
  });

  it("establishes the packaged shell height chain before app CSS loads", () => {
    const appHtml = source("src/app.html");
    expect(appHtml).toMatch(
      /html,\s*body,\s*#medousa-app-root\s*\{[^}]*height:\s*100%/s,
    );
    expect(appHtml).toMatch(/html,\s*body\s*\{[^}]*overflow:\s*hidden/s);
  });

  it("ships one deterministic stylesheet to packaged WebViews", () => {
    const svelte = source("svelte.config.js");
    expect(svelte).toMatch(/bundleStrategy:\s*["']single["']/);

    const liquid = source("src/lib/liquid/archetypes/registerUi.ts");
    expect(liquid).toContain('import "$lib/liquid/styles/liquidOverflow.css"');
    expect(liquid).not.toMatch(/import\([^)]*liquidOverflow\.css/);
  });

  it("keeps benchmark route geometry scoped when CSS is bundled globally", () => {
    const harness = source("src/routes/p02-browser-harness/+page.svelte");
    expect(harness).not.toMatch(/:global\(body\)\s*\{/);
    expect(harness).toContain(".p02-browser-harness-page");
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
