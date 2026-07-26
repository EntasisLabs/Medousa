/**
 * Generate Operator's Guide Liquid catalog chapter from liquidCatalogDemos.ts.
 * Run: npx vite-node src/lib/guide/generateLiquidCatalog.ts
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildLiquidCatalogMarkdown } from "./liquidCatalogDemos";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const outPath =
  process.argv[2] ?? join(root, "src/lib/guide/pages/23-liquid-reference.md");

writeFileSync(outPath, buildLiquidCatalogMarkdown(), "utf8");
console.log(`Wrote ${outPath}`);
