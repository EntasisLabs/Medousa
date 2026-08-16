/**
 * H09 FRONT-001: Vite client manifest inventory for each startup closure.
 *
 * Requires a production build (`npm run build`) so `.svelte-kit/output/client`
 * exists. Regenerating ceilings: npm run check:bundle-budget -- --write
 */
import assert from "node:assert/strict";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const clientRoot = join(homeRoot, ".svelte-kit", "output", "client");
const manifestPath = join(clientRoot, ".vite", "manifest.json");
const budgetPath = join(homeRoot, "security", "bundle-budget.json");

const EAGER_SHELLS = {
  desktop: "src/lib/components/layout/WorkshopShell.svelte",
  mobile: "src/lib/components/mobile/MobileShell.svelte",
};

function loadManifest() {
  try {
    return JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch (error) {
    throw new Error(
      `Vite client manifest missing at ${manifestPath}. Run npm run build first. (${error.message})`,
    );
  }
}

function isRootEntry(key) {
  return (
    /\/client-optimized\/app\.js$/.test(key) ||
    key.endsWith("runtime/client/entry.js") ||
    /\/nodes\/0\.js$/.test(key) ||
    /\/nodes\/2\.js$/.test(key)
  );
}

const DORMANT_OVERLAYS = [
  "CommandSpotlight",
  "WizardContainer",
  "VaultNoteWorkshop",
  "BrowserWorkshop",
  "MobileBrowserWorkshop",
  "WorkAskDockPopover",
  "VaultGarageImportWizard",
  "VaultContextMenu",
  "ScriptContextMenu",
  "ShellContextMenu",
  "VaultAttachmentPanel",
];

function assertPlatformClosure(manifest, jsFiles, platform) {
  const staticFiles = new Set(jsFiles);
  for (const [key, entry] of Object.entries(manifest)) {
    if (!entry?.file || !staticFiles.has(entry.file)) continue;
    if (platform === "desktop") {
      assert.ok(
        !/MobileShell/.test(key),
        `desktop startup closure includes MobileShell via ${key}`,
      );
      assert.ok(
        !/src\/lib\/components\/mobile\//.test(key),
        `desktop startup closure includes mobile destination ${key}`,
      );
    } else {
      assert.ok(
        !/WorkshopShell/.test(key),
        `mobile startup closure includes WorkshopShell via ${key}`,
      );
    }
    for (const name of DORMANT_OVERLAYS) {
      assert.ok(
        !key.includes(name),
        `${platform} startup closure includes dormant overlay ${name} via ${key}`,
      );
    }
  }
}

function walkStatic(manifest, startKeys) {
  const files = new Set();
  const cssFiles = new Set();
  const queue = [...startKeys];
  const seen = new Set();
  while (queue.length > 0) {
    const key = queue.pop();
    if (seen.has(key)) continue;
    seen.add(key);
    const entry = manifest[key];
    if (!entry) continue;
    files.add(entry.file);
    for (const css of entry.css ?? []) cssFiles.add(css);
    for (const imported of entry.imports ?? []) queue.push(imported);
  }
  return { js: [...files], css: [...cssFiles] };
}

function assetBytes(rel) {
  return statSync(join(clientRoot, rel)).size;
}

function assetGzipBytes(rel) {
  return gzipSync(readFileSync(join(clientRoot, rel))).length;
}

function measureFiles(js, css) {
  const jsBytes = js.reduce((sum, file) => sum + assetBytes(file), 0);
  const cssBytes = css.reduce((sum, file) => sum + assetBytes(file), 0);
  const jsGzipBytes = js.reduce((sum, file) => sum + assetGzipBytes(file), 0);
  const cssGzipBytes = css.reduce((sum, file) => sum + assetGzipBytes(file), 0);
  return {
    jsFiles: js.length,
    cssFiles: css.length,
    jsBytes,
    jsGzipBytes,
    cssBytes,
    cssGzipBytes,
    largestJsChunkBytes: Math.max(0, ...js.map((file) => assetBytes(file))),
  };
}

function measurePlatformClosure(manifest, rootKeys, platform) {
  const shellEntry = EAGER_SHELLS[platform];
  assert.ok(manifest[shellEntry], `missing ${platform} shell manifest entry ${shellEntry}`);
  const startKeys = [...rootKeys, shellEntry];
  const { js, css } = walkStatic(manifest, startKeys);
  assertPlatformClosure(manifest, js, platform);
  return {
    shellEntry,
    ...measureFiles(js, css),
  };
}

export function measureStartupClosures(manifest = loadManifest()) {
  const rootEntries = Object.keys(manifest).filter((key) => isRootEntry(key)).sort();
  assert.ok(rootEntries.length > 0, "no root Vite entries found in client manifest");
  return {
    rootEntries,
    desktop: measurePlatformClosure(manifest, rootEntries, "desktop"),
    mobile: measurePlatformClosure(manifest, rootEntries, "mobile"),
  };
}

function main() {
  const measured = measureStartupClosures();
  if (process.argv.includes("--write")) {
    const snapshot = {
      schemaVersion: 2,
      notes:
        "FRONT-001 regression ceilings for the real desktop/mobile startup closures. These are binding regression ratchets, not evidence that the H09 target has been met.",
      manifest: ".svelte-kit/output/client/.vite/manifest.json",
      ceilings: {
        desktop: measured.desktop,
        mobile: measured.mobile,
      },
      measured,
    };
    writeFileSync(budgetPath, `${JSON.stringify(snapshot, null, 2)}\n`);
    console.log(
      `Wrote security/bundle-budget.json: desktop JS ${measured.desktop.jsBytes}, mobile JS ${measured.mobile.jsBytes}`,
    );
    return;
  }
  const expected = JSON.parse(readFileSync(budgetPath, "utf8"));
  assert.equal(expected.schemaVersion, 2);
  for (const platform of Object.keys(EAGER_SHELLS)) {
    for (const key of [
      "jsFiles",
      "cssFiles",
      "jsBytes",
      "jsGzipBytes",
      "cssBytes",
      "cssGzipBytes",
      "largestJsChunkBytes",
    ]) {
      assert.ok(
        measured[platform][key] <= expected.ceilings[platform][key],
        `${platform}.${key} ${measured[platform][key]} exceeds ceiling ${expected.ceilings[platform][key]}`,
      );
    }
  }
  console.log(
    `Bundle budget verified: desktop JS ${measured.desktop.jsBytes} ≤ ${expected.ceilings.desktop.jsBytes}, mobile JS ${measured.mobile.jsBytes} ≤ ${expected.ceilings.mobile.jsBytes}`,
  );
}

const invokedDirectly =
  process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  main();
}
