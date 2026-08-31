import assert from "node:assert/strict";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(root, "src-tauri");
const inventoryPath = join(tauriRoot, "security", "browser-authority-inventory.json");

const readJson = async (path) => JSON.parse(await readFile(path, "utf8"));
const sortedUnique = (values) => [...new Set(values)].sort();

const trusted = await readJson(join(tauriRoot, "capabilities", "default.json"));
const remote = await readJson(join(tauriRoot, "capabilities", "browser-tab-webviews.json"));
const tauriConfig = await readJson(join(tauriRoot, "tauri.conf.json"));
const cargoManifest = await readFile(join(tauriRoot, "Cargo.toml"), "utf8");
const cargoLock = await readFile(join(tauriRoot, "Cargo.lock"), "utf8");
const lib = await readFile(join(tauriRoot, "src", "lib.rs"), "utf8");
const bridge = await readFile(join(tauriRoot, "src", "browser_report_bridge.rs"), "utf8");
const handlerStart = lib.indexOf(".invoke_handler(tauri::generate_handler![");
assert.notEqual(handlerStart, -1, "Tauri application invoke handler was not found");
const handlerEnd = lib.indexOf("\n        ])", handlerStart);
assert.notEqual(handlerEnd, -1, "Tauri application invoke handler end was not found");
const handler = lib.slice(handlerStart, handlerEnd);
const applicationCommands = sortedUnique(
  [...handler.matchAll(/^\s*([A-Za-z_][\w]*(?:::[A-Za-z_][\w]*)+),\s*$/gm)].map(
    ([, command]) => command,
  ),
);
const browserBridgeCommands = sortedUnique(
  [...bridge.matchAll(/#\[tauri::command\]\s+fn\s+([A-Za-z_][\w]*)/g)].map(
    ([, command]) => command,
  ),
);
assert.deepEqual(browserBridgeCommands, ["report"], "browser bridge must expose exactly one command");

const permissionId = (permission) =>
  typeof permission === "string" ? permission : permission.identifier;
const remotePermissions = (remote.permissions ?? []).map(permissionId);
const trustedPermissions = (trusted.permissions ?? []).map(permissionId);
const trustedWebviews = sortedUnique(trusted.webviews ?? []);
const remoteWebviews = sortedUnique(remote.webviews ?? []);

assert.deepEqual(trusted.windows ?? [], [], "trusted grants must target webviews, not parent windows");
assert.equal(remote.local, false, "remote capability must not match local app/custom-protocol pages");
assert.deepEqual(remote.remote?.urls, ["https://*", "http://*"], "remote URLs must stay HTTP(S)-only");
assert.deepEqual(
  remoteWebviews,
  ["browser-content-embed-*", "browser-content-popout"],
  "remote capability labels changed; review their authority before updating the inventory",
);
assert.deepEqual(
  remotePermissions,
  ["browser-bridge:allow-report"],
  "remote webviews may receive only the report-only browser bridge",
);
assert.ok(trustedWebviews.includes("main"), "main shell webview must retain trusted authority");
assert.ok(
  trustedWebviews.includes("browser-chrome"),
  "browser chrome must be explicit and distinct from remote browser content",
);
assert.equal(
  trustedWebviews.some((trustedLabel) =>
    remoteWebviews.some((remoteLabel) => {
      const prefix = remoteLabel.endsWith("*") ? remoteLabel.slice(0, -1) : remoteLabel;
      return trustedLabel === remoteLabel || trustedLabel.startsWith(prefix);
    }),
  ),
  false,
  "trusted and remote webview labels overlap",
);

const csp = tauriConfig.app?.security?.csp;
assert.equal(typeof csp, "string", "trusted shell production CSP must be enabled");
const cspDirectives = Object.fromEntries(
  csp
    .split(";")
    .map((directive) => directive.trim().split(/\s+/))
    .filter(([name]) => name)
    .map(([name, ...sources]) => [name, sources]),
);
for (const directive of [
  "default-src",
  "base-uri",
  "object-src",
  "form-action",
  "frame-ancestors",
  "script-src",
  "style-src",
  "font-src",
  "img-src",
  "connect-src",
  "frame-src",
  "worker-src",
]) {
  assert.ok(cspDirectives[directive], `trusted shell CSP is missing ${directive}`);
}
assert.deepEqual(cspDirectives["default-src"], ["'self'"]);
assert.deepEqual(cspDirectives["object-src"], ["'none'"]);
assert.deepEqual(cspDirectives["base-uri"], ["'self'"]);
assert.deepEqual(cspDirectives["form-action"], ["'self'"]);
assert.deepEqual(cspDirectives["frame-ancestors"], ["'none'"]);
assert.deepEqual(cspDirectives["script-src"], ["'self'", "'wasm-unsafe-eval'"]);
for (const forbidden of ["'unsafe-eval'", "'unsafe-inline'", "http:", "https:", "data:", "blob:", "*"]) {
  assert.ok(!cspDirectives["script-src"].includes(forbidden), `script-src permits ${forbidden}`);
}
assert.equal(
  tauriConfig.app?.security?.assetProtocol?.enable ?? false,
  false,
  "broad Tauri asset protocol must stay disabled",
);
assert.ok(!cargoManifest.includes('"protocol-asset"'), "Tauri asset protocol feature must stay disabled");

const sourceFiles = [];
const collectSources = async (directory) => {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) await collectSources(path);
    else if (/\.(?:ts|js|svelte)$/.test(entry.name)) sourceFiles.push(path);
  }
};
await collectSources(join(root, "src"));
for (const path of sourceFiles) {
  assert.ok(!(await readFile(path, "utf8")).includes("convertFileSrc"), `raw asset URL flow remains in ${path}`);
}

const attackerSources = ["'unsafe-inline'", "'unsafe-eval'", "https:", "http:", "data:", "blob:", "*"];
assert.ok(
  attackerSources.every((source) => !cspDirectives["script-src"].includes(source)),
  "trusted-shell injection fixture escaped script-src",
);

const lockedVersion = (name) => {
  const match = cargoLock.match(
    new RegExp(`\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = "([^"]+)"`),
  );
  assert.ok(match, `${name} is missing from the desktop Cargo.lock`);
  return match[1];
};

try {
  const generatedCapabilities = await readJson(join(tauriRoot, "gen", "schemas", "capabilities.json"));
  const reviewedRemoteCapability = { ...remote };
  delete reviewedRemoteCapability.$schema;
  assert.deepEqual(
    generatedCapabilities[remote.identifier],
    reviewedRemoteCapability,
    "generated remote capability differs from the reviewed source capability",
  );
  const acl = await readJson(join(tauriRoot, "gen", "schemas", "acl-manifests.json"));
  assert.deepEqual(
    acl["browser-bridge"]?.permissions?.["allow-report"]?.commands,
    { allow: ["report"], deny: [] },
    "generated browser bridge ACL is not report-only",
  );
} catch (error) {
  if (error?.code !== "ENOENT") throw error;
}

const inventory = {
  schemaVersion: 2,
  applicationCommands,
  plugins: {
    "browser-bridge": browserBridgeCommands,
  },
  trusted: {
    webviews: trustedWebviews,
    permissions: sortedUnique(trustedPermissions),
  },
  remote: {
    webviews: remoteWebviews,
    urls: remote.remote.urls,
    permissions: sortedUnique(remotePermissions),
  },
  trustedShell: {
    csp,
    assetProtocolEnabled: false,
  },
  lockedRuntime: {
    tauri: lockedVersion("tauri"),
    tauriRuntimeWry: lockedVersion("tauri-runtime-wry"),
    wry: lockedVersion("wry"),
  },
};

if (process.argv.includes("--write")) {
  await writeFile(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`);
  console.log(`Wrote ${inventoryPath}`);
} else {
  const expected = await readJson(inventoryPath);
  assert.deepEqual(
    inventory,
    expected,
    "browser authority inventory changed; review it, then regenerate with npm run check:browser-capabilities -- --write",
  );
  console.log(
    `Browser authority verified: ${applicationCommands.length} application commands, ${remotePermissions.length} remote permissions`,
  );
}
