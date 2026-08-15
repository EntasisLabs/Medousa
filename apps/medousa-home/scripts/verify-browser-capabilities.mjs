import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(root, "src-tauri");
const inventoryPath = join(tauriRoot, "security", "browser-authority-inventory.json");

const readJson = async (path) => JSON.parse(await readFile(path, "utf8"));
const sortedUnique = (values) => [...new Set(values)].sort();

const trusted = await readJson(join(tauriRoot, "capabilities", "default.json"));
const remote = await readJson(join(tauriRoot, "capabilities", "browser-tab-webviews.json"));
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

const inventory = {
  schemaVersion: 1,
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
