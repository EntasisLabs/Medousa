import { build } from "esbuild";
import { cp, mkdir } from "node:fs/promises";

const production = process.argv.includes("production");

await mkdir("dist", { recursive: true });
await Promise.all([
  cp("manifest.json", "dist/manifest.json"),
  cp("src/sidepanel.html", "dist/sidepanel.html"),
  cp("src/styles.css", "dist/styles.css"),
]);

await build({
  entryPoints: {
    background: "src/background.ts",
    sidepanel: "src/sidepanel.ts",
  },
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  outdir: "dist",
  sourcemap: production ? false : "inline",
  minify: production,
  logLevel: "info",
});
