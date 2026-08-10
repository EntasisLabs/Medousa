#!/usr/bin/env node
/**
 * Refresh vendored Material Icon Theme file glyphs + association JSON.
 * Source and license details: ../../THIRD_PARTY_NOTICES.md
 * Run from apps/medousa-home after bumping `material-icon-theme`:
 *   node scripts/sync-file-icons.mjs
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = join(root, "node_modules/material-icon-theme");
const themePath = join(pkg, "dist/material-icons.json");
const iconsSrc = join(pkg, "icons");
const outDir = join(root, "static/file-icons");
const outJson = join(root, "src/lib/code/materialIconTheme.json");

if (!existsSync(themePath)) {
  console.error("Install material-icon-theme first.");
  process.exit(1);
}

const theme = JSON.parse(readFileSync(themePath, "utf8"));

const refs = new Set([theme.file ?? "file"]);
for (const section of ["fileExtensions", "fileNames", "languageIds"]) {
  for (const id of Object.values(theme[section] ?? {})) refs.add(id);
  for (const id of Object.values(theme.light?.[section] ?? {})) refs.add(id);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

let copied = 0;
for (const name of [...refs].sort()) {
  const src = join(iconsSrc, `${name}.svg`);
  if (!existsSync(src)) continue;
  copyFileSync(src, join(outDir, `${name}.svg`));
  copied += 1;
}

writeFileSync(
  outJson,
  JSON.stringify({
    file: theme.file ?? "file",
    fileExtensions: theme.fileExtensions ?? {},
    fileNames: theme.fileNames ?? {},
    languageIds: theme.languageIds ?? {},
  }),
);

console.log(`Synced ${copied} icons → static/file-icons (${readdirSync(outDir).length} files)`);
