/**
 * H09 ARCH-001: first-party static runtime import graph and SCC ledger.
 *
 * Type-only imports are excluded. Dynamic import() is excluded (it must not
 * hide a cycle). Regenerating the ledger: npm run check:runtime-graph -- --write
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ts = createRequire(import.meta.url)("typescript");

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcRoot = join(homeRoot, "src");
const ledgerPath = join(homeRoot, "security", "runtime-scc-ledger.json");

const SOURCE_EXTS = new Set([".ts", ".js", ".svelte", ".svelte.ts", ".svelte.js"]);
const SKIP_NAME = /\.(?:test|spec)\.(?:ts|js)$/;

const posix = (value) => value.split(sep).join("/");

export const toPosix = (abs) => posix(relative(homeRoot, abs));

function isSourceFile(name) {
  if (SKIP_NAME.test(name)) return false;
  if (name.endsWith(".d.ts")) return false;
  if (name.endsWith(".svelte.ts") || name.endsWith(".svelte.js")) return true;
  const ext = extname(name);
  return ext === ".ts" || ext === ".js" || ext === ".svelte";
}

function walkSources(dir, files = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === ".svelte-kit") continue;
      walkSources(path, files);
      continue;
    }
    if (isSourceFile(entry.name)) files.push(path);
  }
  return files;
}

function extractSvelteScripts(source) {
  const scripts = [];
  const pattern = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
  let match;
  while ((match = pattern.exec(source))) {
    scripts.push(match[2] ?? "");
  }
  return scripts;
}

function parseableText(absPath, source) {
  if (absPath.endsWith(".svelte")) {
    return extractSvelteScripts(source).join("\n");
  }
  return source;
}

function namedImportsAreTypeOnly(clause) {
  if (!clause || clause.name) return false;
  if (!clause.namedBindings || !ts.isNamedImports(clause.namedBindings)) return false;
  return clause.namedBindings.elements.every((element) => element.isTypeOnly);
}

function collectModuleSpecifiers(sf) {
  const specs = [];
  const visit = (node) => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      const clause = node.importClause;
      if (clause?.isTypeOnly || namedImportsAreTypeOnly(clause)) {
        ts.forEachChild(node, visit);
        return;
      }
      specs.push(node.moduleSpecifier.text);
    } else if (
      ts.isExportDeclaration(node) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      if (!node.isTypeOnly) specs.push(node.moduleSpecifier.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return specs;
}

function candidateFiles(resolvedWithoutExt) {
  return [
    resolvedWithoutExt,
    `${resolvedWithoutExt}.ts`,
    `${resolvedWithoutExt}.js`,
    `${resolvedWithoutExt}.svelte.ts`,
    `${resolvedWithoutExt}.svelte.js`,
    `${resolvedWithoutExt}.svelte`,
    join(resolvedWithoutExt, "index.ts"),
    join(resolvedWithoutExt, "index.js"),
    join(resolvedWithoutExt, "index.svelte.ts"),
    join(resolvedWithoutExt, "index.svelte"),
  ];
}

export function resolveSpecifier(fromFile, specifier, root = srcRoot) {
  let base;
  if (specifier.startsWith("$lib/")) {
    base = join(root, "lib", specifier.slice("$lib/".length));
  } else if (specifier === "$lib") {
    base = join(root, "lib");
  } else if (specifier.startsWith("./") || specifier.startsWith("../")) {
    base = resolve(dirname(fromFile), specifier);
  } else {
    return null;
  }
  for (const candidate of candidateFiles(base)) {
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {
      /* missing */
    }
  }
  return null;
}

export function buildRuntimeGraph(root = srcRoot) {
  const files = walkSources(root);
  const fileSet = new Set(files.map((file) => resolve(file)));
  /** @type {Map<string, string[]>} */
  const graph = new Map();
  const unresolved = [];

  for (const abs of files) {
    const source = readFileSync(abs, "utf8");
    const text = parseableText(abs, source);
    const sf = ts.createSourceFile(
      abs.endsWith(".svelte") ? `${abs}.ts` : abs,
      text,
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TS,
    );
    const edges = [];
    for (const spec of collectModuleSpecifiers(sf)) {
      const resolved = resolveSpecifier(abs, spec, root);
      if (!resolved) {
        if (spec.startsWith("$lib") || spec.startsWith(".") ) {
          unresolved.push({ from: toPosix(abs), specifier: spec });
        }
        continue;
      }
      const resolvedAbs = resolve(resolved);
      if (!fileSet.has(resolvedAbs)) continue;
      edges.push(resolvedAbs);
    }
    graph.set(resolve(abs), [...new Set(edges)].sort());
  }

  return { graph, unresolved };
}

export function stronglyConnectedComponents(graph) {
  let index = 0;
  const indices = new Map();
  const lowlink = new Map();
  const onStack = new Set();
  const stack = [];
  const sccs = [];

  const connect = (v) => {
    indices.set(v, index);
    lowlink.set(v, index);
    index += 1;
    stack.push(v);
    onStack.add(v);
    for (const w of graph.get(v) ?? []) {
      if (!graph.has(w)) continue;
      if (!indices.has(w)) {
        connect(w);
        lowlink.set(v, Math.min(lowlink.get(v), lowlink.get(w)));
      } else if (onStack.has(w)) {
        lowlink.set(v, Math.min(lowlink.get(v), indices.get(w)));
      }
    }
    if (lowlink.get(v) === indices.get(v)) {
      const component = [];
      let w;
      do {
        w = stack.pop();
        onStack.delete(w);
        component.push(w);
      } while (w !== v);
      sccs.push(component);
    }
  };

  for (const v of graph.keys()) {
    if (!indices.has(v)) connect(v);
  }
  return sccs;
}

function shortestCycle(graph, members) {
  const memberSet = new Set(members);
  if (members.length === 1) {
    const self = graph.get(members[0]) ?? [];
    return self.includes(members[0]) ? [toPosix(members[0]), toPosix(members[0])] : [];
  }
  let best = null;
  for (const start of members) {
    const queue = [[start]];
    const seen = new Set([start]);
    while (queue.length > 0) {
      const path = queue.shift();
      const last = path[path.length - 1];
      for (const next of graph.get(last) ?? []) {
        if (!memberSet.has(next)) continue;
        if (next === start && path.length >= 2) {
          const cycle = [...path, start];
          if (!best || cycle.length < best.length) best = cycle;
          continue;
        }
        if (seen.has(next) || path.includes(next)) continue;
        seen.add(next);
        queue.push([...path, next]);
      }
    }
  }
  return (best ?? []).map(toPosix);
}

function familyId(membersPosix, shortest) {
  const hay = `${membersPosix.join("\n")}\n${shortest.join("\n")}`;
  const rules = [
    ["markdown-liquid-vault", /markdown\/|liquid\/archetypes|hydrateLiquid/],
    ["lme-shell-undertakings", /lmeWorkspace|shellTabs|undertakings/],
    ["vault-space-config", /vaultSpaces|vaultTemplates|customSpaces/],
    ["human-browser", /humanBrowser|browserHistory|browser\/.*store/],
    ["voice-workshop-defaults", /voicePresets|workshopDefaults/],
    ["identity-profiles", /identity\.svelte|userProfiles/],
    ["browser-compositor-popover", /browserCompositor|browserPopover/],
  ];
  for (const [id, pattern] of rules) {
    if (pattern.test(hay)) return id;
  }
  return `scc-${membersPosix.length}-${membersPosix[0]?.replaceAll("/", "_") ?? "empty"}`;
}

export function ledgerFromGraph(graph) {
  const cyclic = stronglyConnectedComponents(graph)
    .filter((members) => members.length > 1)
    .map((members) => {
      const sortedAbs = [...members].sort();
      const posixMembers = sortedAbs.map(toPosix).sort();
      const cycle = shortestCycle(graph, sortedAbs);
      return {
        id: familyId(posixMembers, cycle),
        size: posixMembers.length,
        members: posixMembers,
        shortestCycle: cycle,
      };
    })
    .sort((a, b) => b.size - a.size || a.id.localeCompare(b.id));

  const used = new Map();
  for (const scc of cyclic) {
    const count = used.get(scc.id) ?? 0;
    used.set(scc.id, count + 1);
    if (count > 0) scc.id = `${scc.id}-${count + 1}`;
  }

  return {
    schemaVersion: 1,
    notes:
      "H09 ARCH-001 migration ledger. Zero new first-party runtime SCCs. Burn-down requires --write.",
    sccs: cyclic,
  };
}

function fingerprint(ledger) {
  return ledger.sccs.map((scc) => ({
    id: scc.id,
    members: [...scc.members].sort(),
  }));
}

function assertLedgerMatch(actual, expected) {
  assert.equal(actual.schemaVersion, expected.schemaVersion, "ledger schemaVersion changed");
  const actualSets = fingerprint(actual);
  const expectedSets = fingerprint(expected);
  const extra = actualSets.filter(
    (got) =>
      !expectedSets.some(
        (want) =>
          want.members.length === got.members.length &&
          want.members.every((member, i) => member === got.members[i]),
      ),
  );
  const missing = expectedSets.filter(
    (want) =>
      !actualSets.some(
        (got) =>
          want.members.length === got.members.length &&
          want.members.every((member, i) => member === got.members[i]),
      ),
  );
  assert.deepEqual(
    extra,
    [],
    `new first-party runtime SCCs (update only with --write after review):\n${JSON.stringify(extra, null, 2)}`,
  );
  assert.deepEqual(
    missing,
    [],
    `ledger SCCs disappeared; regenerate with npm run check:runtime-graph -- --write:\n${JSON.stringify(missing, null, 2)}`,
  );
}

function runSelfTest() {
  const dir = mkdtempSync(join(tmpdir(), "h09-runtime-graph-"));
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "a.ts"), `import "./b";\n`);
    writeFileSync(join(dir, "b.ts"), `import "./a";\n`);
    const { graph } = buildRuntimeGraph(dir);
    const ledger = ledgerFromGraph(graph);
    assert.equal(ledger.sccs.length, 1, "synthetic graph must contain exactly one SCC");
    assert.equal(ledger.sccs[0].size, 2);
    const members = new Set(ledger.sccs[0].members.map((member) => member.split("/").pop()));
    assert.ok(members.has("a.ts") && members.has("b.ts"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function main() {
  runSelfTest();
  const { graph, unresolved } = buildRuntimeGraph();
  const actual = ledgerFromGraph(graph);
  if (process.argv.includes("--write")) {
    writeFileSync(ledgerPath, `${JSON.stringify(actual, null, 2)}\n`);
    console.log(
      `Wrote ${toPosix(ledgerPath)}: ${actual.sccs.length} SCCs, largest ${actual.sccs[0]?.size ?? 0}`,
    );
    if (unresolved.length > 0) {
      console.warn(`Unresolved first-party specifiers: ${unresolved.length}`);
    }
    return;
  }
  const expected = JSON.parse(readFileSync(ledgerPath, "utf8"));
  assertLedgerMatch(actual, expected);
  console.log(
    `Runtime graph verified: ${actual.sccs.length} first-party SCCs, largest ${actual.sccs[0]?.size ?? 0} modules`,
  );
  if (unresolved.length > 0) {
    console.warn(`Unresolved first-party specifiers: ${unresolved.length}`);
    for (const item of unresolved.slice(0, 8)) {
      console.warn(`  ${item.from} -> ${item.specifier}`);
    }
  }
}

const invokedDirectly = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  main();
}
