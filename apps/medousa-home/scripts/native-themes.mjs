#!/usr/bin/env node
/**
 * Write the resolved theme catalog the native (RNDR) shell embeds.
 * `--check` exits non-zero when the committed file is stale.
 */
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { buildSync } from "esbuild";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(homeRoot, "../medousa-native/themes/medousa-themes.json");
const check = process.argv.includes("--check");

const outfile = join(homeRoot, ".svelte-kit/medousa-native-themes.mjs");
mkdirSync(dirname(outfile), { recursive: true });
buildSync({
  absWorkingDir: homeRoot,
  entryPoints: [join(homeRoot, "themes/native-themes.ts")],
  outfile,
  bundle: true,
  format: "esm",
  platform: "node",
  packages: "external",
  logLevel: "silent",
});
const { syncNativeThemes } = await import(`${pathToFileURL(outfile).href}?t=${Date.now()}`);
if (!syncNativeThemes(target, check)) {
  console.error(`${target} is stale. Run: npm run themes:native`);
  process.exit(1);
}
console.log(check ? "native themes are current" : `wrote ${target}`);
