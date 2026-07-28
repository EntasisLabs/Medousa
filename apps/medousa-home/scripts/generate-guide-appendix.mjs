#!/usr/bin/env node
/**
 * Generate Operator's Guide command appendix from source catalogs.
 * Run: node scripts/generate-guide-appendix.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outPath =
  process.argv[2] ?? join(root, "src/lib/guide/pages/24-commands-reference.md");

function readRel(rel) {
  return readFileSync(join(root, rel), "utf8");
}

function formatKeysMac(keys) {
  if (keys.startsWith("literal:")) return keys.slice("literal:".length);
  if (keys.startsWith("prefix:")) return `⌘; then ${keys.slice("prefix:".length)}`;
  if (keys.startsWith("mod:")) {
    const chord = keys.slice("mod:".length);
    if (chord.includes(" / ")) {
      return chord
        .split(" / ")
        .map((part) => formatModChordMac(part.trim()))
        .join(" / ");
    }
    return formatModChordMac(chord);
  }
  return keys;
}

function formatKeysWin(keys) {
  if (keys.startsWith("literal:")) return keys.slice("literal:".length);
  if (keys.startsWith("prefix:")) return `Ctrl+; then ${keys.slice("prefix:".length)}`;
  if (keys.startsWith("mod:")) {
    const chord = keys.slice("mod:".length);
    if (chord.includes(" / ")) {
      return chord
        .split(" / ")
        .map((part) => formatModChordWin(part.trim()))
        .join(" / ");
    }
    return formatModChordWin(chord);
  }
  return keys;
}

function formatModChordMac(chord) {
  if (chord.startsWith("Shift+")) return `⇧⌘${chord.slice("Shift+".length)}`;
  return `⌘${chord}`;
}

function formatModChordWin(chord) {
  if (chord.startsWith("Shift+")) return `Ctrl+Shift+${chord.slice("Shift+".length)}`;
  return `Ctrl+${chord}`;
}

function parseKeyboardCatalog(src) {
  const groups = [];
  const groupRe = /id:\s*"([^"]+)",\s*title:\s*"([^"]+)",\s*entries:\s*\[([\s\S]*?)\n\s*\],/g;
  let gm;
  while ((gm = groupRe.exec(src))) {
    const [, id, title, body] = gm;
    const entries = [];
    const entryRe =
      /\{\s*id:\s*"([^"]+)",\s*keys:\s*(?:"([^"]+)"|'([^']+)'),\s*action:\s*"([^"]+)",?\s*\}/g;
    let em;
    while ((em = entryRe.exec(body))) {
      entries.push({ id: em[1], keys: em[2] ?? em[3], action: em[4] });
    }
    groups.push({ id, title, entries });
  }
  return groups;
}

function parseSlashHints(src) {
  const m = src.match(/export const SLASH_COMMAND_HINTS = \[([\s\S]*?)\];/);
  if (!m) return [];
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
}

function parseGoDestinations(src) {
  const m = src.match(
    /const GO_DESTINATIONS[\s\S]*?= \[([\s\S]*?)\];/,
  );
  if (!m) return [];
  const rows = [];
  const re =
    /\{\s*surface:\s*"([^"]+)",\s*label:\s*"([^"]+)",\s*subtitle:\s*"([^"]+)",/g;
  let match;
  while ((match = re.exec(m[1]))) {
    rows.push({ surface: match[1], label: match[2], subtitle: match[3] });
  }
  return rows;
}

/** Static Spotlight commands: id + label + subtitle on nearby lines. */
function parseSpotlightCommands(src) {
  const rows = [];
  const re =
    /id:\s*"([a-z0-9-.:]+)",\s*\n(?:\s*(?:kind|group|section|when|disabled|icon|verb|risk|advanced|aliases|keywords|hint)[^\n]*\n)*\s*label:\s*(?:"([^"]+)"|`([^`]+)`),\s*\n(?:\s*(?:kind|group|section|when|disabled|icon|verb|risk|advanced|aliases|keywords|hint)[^\n]*\n)*\s*subtitle:\s*(?:"([^"]*)"|`([^`]*)`)/g;
  let match;
  while ((match = re.exec(src))) {
    const id = match[1];
    if (id.startsWith("go-") || id.startsWith("workspace-switch-")) continue;
    if (id.includes("${")) continue;
    const label = (match[2] ?? match[3] ?? "").replace(/\$\{[^}]+\}/g, "…");
    const subtitle = (match[4] ?? match[5] ?? "").replace(/\$\{[^}]+\}/g, "…");
    if (!label) continue;
    rows.push({ id, label, subtitle });
  }
  // Dedupe by id
  const seen = new Set();
  return rows.filter((r) => {
    if (seen.has(r.id)) return false;
    seen.add(r.id);
    return true;
  });
}

const catalogSrc = readRel("src/lib/utils/keyboardShortcutsCatalog.ts");
const slashSrc = readRel("src/lib/utils/slashCommands.ts");
const registrySrc = readRel("src/lib/commands/registry.ts");
const doCommandsSrc = readRel("src/lib/commands/doCommands.ts");
const pinCommandsSrc = readRel("src/lib/commands/pinCommands.ts");

const groups = parseKeyboardCatalog(catalogSrc);
const slashHints = parseSlashHints(slashSrc);
const goDests = parseGoDestinations(registrySrc);
const spotlight = [
  ...parseSpotlightCommands(registrySrc),
  ...parseSpotlightCommands(doCommandsSrc),
  ...parseSpotlightCommands(pinCommandsSrc),
];
// Dedupe across files
{
  const seen = new Set();
  for (let i = spotlight.length - 1; i >= 0; i -= 1) {
    if (seen.has(spotlight[i].id)) spotlight.splice(i, 1);
    else seen.add(spotlight[i].id);
  }
}

const lines = [];
const push = (s = "") => lines.push(s);

push("# Commands and keyboard reference");
push("");
push(
  "Full list of keyboard shortcuts and Spotlight commands. Shortcuts can’t be remapped in Settings — these bindings are fixed.",
);
push("");
push(
  "Related: [Keyboard and flow](guide:keyboard-flow) · [Chat](guide:chat) · [Browser and web research](guide:browser)",
);
push("");
push("```callout");
push("tone: note");
push("title: Prefix chord");
push(
  "body: Pane bindings use a prefix — ⌘; on macOS, Ctrl+; elsewhere — then the key (for example ⌘; then % to split right). Spotlight always opens with ⌘K / Ctrl+K.",
);
push("```");
push("");
push("## Keyboard shortcuts");
push("");

for (const group of groups) {
  push(`### ${group.title}`);
  push("");
  push("| Action | macOS | Windows / Linux |");
  push("|--------|-------|-----------------|");
  for (const entry of group.entries) {
    push(
      `| ${entry.action} | ${formatKeysMac(entry.keys)} | ${formatKeysWin(entry.keys)} |`,
    );
  }
  push("");
}

push("Browser chords (also in Spotlight when Web is focused) include address bar, new/close/reopen tab, bookmarks, find, and open external — see Spotlight list below and [Browser](guide:browser).");
push("");
push("## Composer slash commands");
push("");
push("| Command |");
push("|---------|");
for (const hint of slashHints) {
  push(`| \`${hint.split(" — ")[0]}\` — ${hint.split(" — ").slice(1).join(" — ")} |`);
}
push("");
push("## Spotlight — Go destinations");
push("");
push("| Destination | Subtitle |");
push("|-------------|----------|");
for (const d of goDests) {
  push(`| ${d.label} | ${d.subtitle} |`);
}
push("| Channels | Telegram, Discord, Slack — Settings → Sharing |");
push("| MCP connections | Manage MCP servers in Settings → MCP |");
push("");
push("## Spotlight — commands");
push("");
push(
  "Contextual commands (rename desktop, per-desktop switch, etc.) appear when relevant. Static catalog:",
);
push("");
push("| Command | Notes |");
push("|---------|-------|");
for (const cmd of spotlight) {
  const notes = cmd.subtitle.replace(/\|/g, "\\|");
  push(`| ${cmd.label} | ${notes} |`);
}
push("");
push("---");
push("");
push(
  "*This list matches the current app. When something is missing from Spotlight, search by name — contextual commands appear only when they apply.*",
);
push("");

writeFileSync(outPath, lines.join("\n"), "utf8");
console.log(`Wrote ${outPath} (${groups.length} shortcut groups, ${spotlight.length} spotlight cmds)`);
