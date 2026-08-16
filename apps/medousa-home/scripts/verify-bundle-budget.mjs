/**
 * H09 FRONT-001: Vite client manifest inventory for the root static closure.
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

function assertNoMobileDestinations(manifest, jsFiles) {
  const staticFiles = new Set(jsFiles);
  for (const [key, entry] of Object.entries(manifest)) {
    if (!entry?.file || !staticFiles.has(entry.file)) continue;
    assert.ok(
      !/MobileShell/.test(key),
      `desktop static closure includes MobileShell via ${key}`,
    );
    assert.ok(
      !/src\/lib\/components\/mobile\//.test(key),
      `desktop static closure includes mobile destination ${key}`,
    );
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

export function measureRootClosure(manifest = loadManifest()) {
  const startKeys = Object.keys(manifest).filter((key) => isRootEntry(key));
  assert.ok(startKeys.length > 0, "no root Vite entries found in client manifest");
  const { js, css } = walkStatic(manifest, startKeys);
  assertNoMobileDestinations(manifest, js);
  const jsBytes = js.reduce((sum, file) => sum + assetBytes(file), 0);
  const cssBytes = css.reduce((sum, file) => sum + assetBytes(file), 0);
  const jsGzip = js.reduce((sum, file) => sum + assetGzipBytes(file), 0);
  const cssGzip = css.reduce((sum, file) => sum + assetGzipBytes(file), 0);
  const largestJs = Math.max(0, ...js.map((file) => assetBytes(file)));
  return {
    rootEntries: startKeys.sort(),
    jsFiles: js.length,
    cssFiles: css.length,
    rootStaticJsBytes: jsBytes,
    rootStaticJsGzipBytes: jsGzip,
    rootStaticCssBytes: cssBytes,
    rootStaticCssGzipBytes: cssGzip,
    largestInitialJsChunkBytes: largestJs,
  };
}

function main() {
  const measured = measureRootClosure();
  if (process.argv.includes("--write")) {
    const snapshot = {
      schemaVersion: 1,
      notes:
        "FRONT-001 regression ceiling from production Vite client manifest. Raise only with review; splits should lower it.",
      manifest: ".svelte-kit/output/client/.vite/manifest.json",
      ceilings: {
        rootStaticJsBytes: measured.rootStaticJsBytes,
        rootStaticJsGzipBytes: measured.rootStaticJsGzipBytes,
        rootStaticCssBytes: measured.rootStaticCssBytes,
        rootStaticCssGzipBytes: measured.rootStaticCssGzipBytes,
        largestInitialJsChunkBytes: measured.largestInitialJsChunkBytes,
      },
      measured,
    };
    writeFileSync(budgetPath, `${JSON.stringify(snapshot, null, 2)}\n`);
    console.log(
      `Wrote security/bundle-budget.json: JS ${measured.rootStaticJsBytes} / gzip ${measured.rootStaticJsGzipBytes}, CSS ${measured.rootStaticCssBytes}, largest JS ${measured.largestInitialJsChunkBytes}`,
    );
    return;
  }
  const expected = JSON.parse(readFileSync(budgetPath, "utf8"));
  assert.equal(expected.schemaVersion, 1);
  for (const key of Object.keys(expected.ceilings)) {
    assert.ok(
      measured[key] <= expected.ceilings[key],
      `${key} ${measured[key]} exceeds ceiling ${expected.ceilings[key]}`,
    );
  }
  console.log(
    `Bundle budget verified: JS ${measured.rootStaticJsBytes} ≤ ${expected.ceilings.rootStaticJsBytes}, CSS ${measured.rootStaticCssBytes} ≤ ${expected.ceilings.rootStaticCssBytes}`,
  );
}

const invokedDirectly =
  process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  main();
}
