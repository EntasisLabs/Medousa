/** H09 ARCH-002: source-size review alarms and binding legacy growth ceilings. */
import assert from "node:assert/strict";
import {
  existsSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcRoot = join(homeRoot, "src");
const ledgerPath = join(homeRoot, "security", "source-ownership-ledger.json");
const REVIEW_LINES = 1_000;
const BLOCKING_LINES = 2_000;
const SOURCE_EXTS = new Set([".ts", ".js", ".svelte", ".postcss", ".css"]);

const toPosix = (path) => relative(homeRoot, path).split(sep).join("/");

function sources(dir, output = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      sources(path, output);
    } else if (
      SOURCE_EXTS.has(extname(entry.name)) &&
      !/\.(?:test|spec)\.(?:ts|js)$/.test(entry.name) &&
      !entry.name.endsWith(".d.ts")
    ) {
      output.push(path);
    }
  }
  return output;
}

function lineCount(path) {
  const source = readFileSync(path, "utf8");
  return source === "" ? 0 : source.split(/\r?\n/).length;
}

function inventory() {
  return sources(srcRoot)
    .map((path) => ({ path: toPosix(path), lines: lineCount(path) }))
    .filter((entry) => entry.lines > REVIEW_LINES)
    .sort((a, b) => b.lines - a.lines || a.path.localeCompare(b.path));
}

function defaultOwner(path) {
  if (path.includes("/vault")) return "Home / Vault";
  if (path.includes("/chat")) return "Home / Chat";
  if (path.includes("/work") || path.includes("forge")) return "Home / Forge";
  if (path.endsWith(".postcss") || path.endsWith(".css")) return "Home / CSS";
  return "Home";
}

function writeLedger(measured) {
  const previous = existsSync(ledgerPath)
    ? JSON.parse(readFileSync(ledgerPath, "utf8"))
    : { alarms: [] };
  const priorByPath = new Map(previous.alarms.map((entry) => [entry.path, entry]));
  const alarms = measured.map(({ path, lines }) => {
    const prior = priorByPath.get(path);
    return {
      path,
      maxLines: lines,
      owner: prior?.owner ?? defaultOwner(path),
      reason:
        prior?.reason ??
        "Legacy H09 ownership debt; growth is forbidden until the owner boundary is split.",
      expires: prior?.expires ?? "2026-10-01",
      blocksValidation: lines > BLOCKING_LINES,
    };
  });
  writeFileSync(
    ledgerPath,
    `${JSON.stringify({
      schemaVersion: 1,
      notes:
        "Review alarms are regression ceilings, not size exceptions. Entries above 2,000 lines block ARCH-002 validation.",
      reviewThresholdLines: REVIEW_LINES,
      blockingThresholdLines: BLOCKING_LINES,
      alarms,
    }, null, 2)}\n`,
  );
}

function main() {
  const measured = inventory();
  if (process.argv.includes("--write")) {
    writeLedger(measured);
    console.log(`Wrote ${toPosix(ledgerPath)} with ${measured.length} ownership alarms`);
    return;
  }
  const ledger = JSON.parse(readFileSync(ledgerPath, "utf8"));
  assert.equal(ledger.schemaVersion, 1);
  assert.equal(ledger.reviewThresholdLines, REVIEW_LINES);
  assert.equal(ledger.blockingThresholdLines, BLOCKING_LINES);
  const measuredByPath = new Map(measured.map((entry) => [entry.path, entry.lines]));
  const ledgerByPath = new Map(ledger.alarms.map((entry) => [entry.path, entry]));

  const newAlarms = measured.filter((entry) => !ledgerByPath.has(entry.path));
  assert.deepEqual(newAlarms, [], `new files exceed ${REVIEW_LINES} lines:\n${JSON.stringify(newAlarms, null, 2)}`);
  for (const entry of ledger.alarms) {
    assert.ok(entry.owner?.trim(), `${entry.path} lacks an owner`);
    assert.ok(entry.reason?.trim(), `${entry.path} lacks a reason`);
    assert.match(entry.expires ?? "", /^\d{4}-\d{2}-\d{2}$/, `${entry.path} lacks an expiry`);
    const lines = measuredByPath.get(entry.path);
    assert.ok(lines !== undefined, `${entry.path} fell below the alarm; regenerate the ledger`);
    assert.ok(lines <= entry.maxLines, `${entry.path} grew to ${lines} lines (ceiling ${entry.maxLines})`);
    assert.equal(
      entry.blocksValidation,
      lines > BLOCKING_LINES,
      `${entry.path} validation flag is stale; regenerate the ledger`,
    );
  }

  const blocking = ledger.alarms.filter((entry) => entry.blocksValidation).length;
  console.log(
    `Source ownership verified: ${ledger.alarms.length} review alarms, ${blocking} still block ARCH-002 validation`,
  );
}

const invokedDirectly = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) main();
