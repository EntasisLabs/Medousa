/**
 * H09 ARCH-001: first-party runtime import graph and boundary ledger.
 *
 * Type-only imports are excluded. Dynamic imports are inventoried separately
 * and cannot hide a forbidden lower-to-higher dependency. Regenerating the
 * ledger: npm run check:runtime-graph -- --write
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
  const staticSpecs = [];
  const dynamicSpecs = [];
  const sideEffectSpecs = [];
  const visit = (node) => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      const clause = node.importClause;
      if (clause?.isTypeOnly || namedImportsAreTypeOnly(clause)) {
        ts.forEachChild(node, visit);
        return;
      }
      staticSpecs.push(node.moduleSpecifier.text);
      if (!clause) sideEffectSpecs.push(node.moduleSpecifier.text);
    } else if (
      ts.isExportDeclaration(node) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      if (!node.isTypeOnly) staticSpecs.push(node.moduleSpecifier.text);
    } else if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword &&
      node.arguments.length === 1 &&
      ts.isStringLiteral(node.arguments[0])
    ) {
      dynamicSpecs.push(node.arguments[0].text);
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return { staticSpecs, dynamicSpecs, sideEffectSpecs };
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
  const dynamicGraph = new Map();
  const sideEffectImports = [];
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
    const dynamicEdges = [];
    const { staticSpecs, dynamicSpecs, sideEffectSpecs } = collectModuleSpecifiers(sf);
    for (const spec of staticSpecs) {
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
    for (const spec of dynamicSpecs) {
      const resolved = resolveSpecifier(abs, spec, root);
      if (!resolved) {
        if (spec.startsWith("$lib") || spec.startsWith(".")) {
          unresolved.push({ from: toPosix(abs), specifier: spec });
        }
        continue;
      }
      const resolvedAbs = resolve(resolved);
      if (fileSet.has(resolvedAbs)) dynamicEdges.push(resolvedAbs);
    }
    for (const specifier of sideEffectSpecs) {
      sideEffectImports.push({ from: toPosix(abs), specifier });
    }
    graph.set(resolve(abs), [...new Set(edges)].sort());
    dynamicGraph.set(resolve(abs), [...new Set(dynamicEdges)].sort());
  }

  return {
    graph,
    dynamicGraph,
    sideEffectImports: sideEffectImports.sort((a, b) =>
      a.from.localeCompare(b.from) || a.specifier.localeCompare(b.specifier)),
    unresolved,
  };
}

const INWARD_BOUNDARY_RULES = [
  {
    id: "types-do-not-import-runtime-ui",
    from: /^src\/lib\/types\//,
    to: /^src\/lib\/(components|stores|runtime)\//,
  },
  {
    id: "config-does-not-import-views",
    from: /^src\/lib\/config\//,
    to: /^src\/lib\/components\//,
  },
  {
    id: "stores-do-not-import-views",
    from: /^src\/lib\/stores\//,
    to: /^src\/lib\/components\//,
  },
  {
    id: "feature-contracts-stay-dependency-light",
    from: /^src\/lib\/runtime\/features\/(types|catalog)\.ts$/,
    to: /^src\/lib\/(components|stores)\//,
  },
];

function boundaryViolations(graph, edgeKind) {
  const violations = [];
  for (const [from, edges] of graph) {
    const fromPath = toPosix(from);
    for (const to of edges) {
      const toPath = toPosix(to);
      for (const rule of INWARD_BOUNDARY_RULES) {
        if (rule.from.test(fromPath) && rule.to.test(toPath)) {
          violations.push({ rule: rule.id, edgeKind, from: fromPath, to: toPath });
        }
      }
    }
  }
  return violations.sort((a, b) =>
    a.rule.localeCompare(b.rule) || a.from.localeCompare(b.from) || a.to.localeCompare(b.to));
}

function crossStoreEdges(graph) {
  const edges = [];
  for (const [from, targets] of graph) {
    const fromPath = toPosix(from);
    if (!fromPath.startsWith("src/lib/stores/")) continue;
    for (const to of targets) {
      const toPath = toPosix(to);
      if (toPath.startsWith("src/lib/stores/") && toPath !== fromPath) {
        edges.push(`${fromPath} -> ${toPath}`);
      }
    }
  }
  return [...new Set(edges)].sort();
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

export function ledgerFromGraph(graph, inventory = {}) {
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
    schemaVersion: 2,
    notes:
      "H09 ARCH-001 migration ledger. Zero new SCCs, boundary violations, or cross-store edges. Existing cross-store edges are burn-down debt, not validation evidence.",
    sccs: cyclic,
    crossStoreEdges: crossStoreEdges(graph),
    sideEffectImports: inventory.sideEffectImports ?? [],
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
  const extraStoreEdges = actual.crossStoreEdges.filter(
    (edge) => !expected.crossStoreEdges.includes(edge),
  );
  const removedStoreEdges = expected.crossStoreEdges.filter(
    (edge) => !actual.crossStoreEdges.includes(edge),
  );
  assert.deepEqual(
    extraStoreEdges,
    [],
    `new cross-store runtime edges are forbidden:\n${extraStoreEdges.join("\n")}`,
  );
  assert.deepEqual(
    removedStoreEdges,
    [],
    `cross-store edges disappeared; regenerate the burn-down ledger with --write:\n${removedStoreEdges.join("\n")}`,
  );
  assert.deepEqual(
    actual.sideEffectImports,
    expected.sideEffectImports,
    "side-effect import inventory changed; remove the side effect or regenerate after review",
  );
}

function runSelfTest() {
  const dir = mkdtempSync(join(tmpdir(), "h09-runtime-graph-"));
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "a.ts"), `import "./b";\n`);
    writeFileSync(join(dir, "b.ts"), `import "./a";\n`);
    writeFileSync(join(dir, "c.ts"), `export const load = () => import("./a");\n`);
    const { graph, dynamicGraph } = buildRuntimeGraph(dir);
    const ledger = ledgerFromGraph(graph);
    assert.equal(ledger.sccs.length, 1, "synthetic graph must contain exactly one SCC");
    assert.equal(ledger.sccs[0].size, 2);
    const members = new Set(ledger.sccs[0].members.map((member) => member.split("/").pop()));
    assert.ok(members.has("a.ts") && members.has("b.ts"));
    assert.deepEqual(dynamicGraph.get(resolve(join(dir, "c.ts"))), [resolve(join(dir, "a.ts"))]);
    const syntheticViolation = new Map([
      [
        join(homeRoot, "src/lib/types/bad.ts"),
        [join(homeRoot, "src/lib/stores/bad.svelte.ts")],
      ],
    ]);
    assert.equal(boundaryViolations(syntheticViolation, "static").length, 1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function main() {
  runSelfTest();
  const { graph, dynamicGraph, sideEffectImports, unresolved } = buildRuntimeGraph();
  const actual = ledgerFromGraph(graph, { sideEffectImports });
  const violations = [
    ...boundaryViolations(graph, "static"),
    ...boundaryViolations(dynamicGraph, "dynamic"),
  ];
  assert.deepEqual(
    violations,
    [],
    `runtime dependency direction violations:\n${JSON.stringify(violations, null, 2)}`,
  );
  assert.deepEqual(
    unresolved,
    [],
    `unresolved first-party runtime imports make the graph incomplete:\n${JSON.stringify(unresolved, null, 2)}`,
  );
  if (process.argv.includes("--write")) {
    writeFileSync(ledgerPath, `${JSON.stringify(actual, null, 2)}\n`);
    console.log(
      `Wrote ${toPosix(ledgerPath)}: ${actual.sccs.length} SCCs, largest ${actual.sccs[0]?.size ?? 0}`,
    );
    return;
  }
  const expected = JSON.parse(readFileSync(ledgerPath, "utf8"));
  assertLedgerMatch(actual, expected);
  console.log(
    `Runtime graph verified: ${actual.sccs.length} SCCs, 0 boundary violations, ${actual.crossStoreEdges.length} grandfathered cross-store edges, ${actual.sideEffectImports.length} side-effect imports`,
  );
}

const invokedDirectly = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  main();
}
