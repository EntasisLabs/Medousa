#!/usr/bin/env node
/**
 * Run a release script by basename on Unix (bash) or Windows (PowerShell).
 *
 * Usage: node scripts/release/release-runner.mjs build.ps1 build.sh -- [args...]
 *
 * Passes the same --flag arguments to both platforms (build.ps1 mirrors build.sh).
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const dashDash = argv.indexOf("--");
const [winScript, unixScript] = argv;
const args = dashDash === -1 ? argv.slice(2) : argv.slice(dashDash + 1);

if (!winScript || !unixScript) {
  console.error("usage: release-runner.mjs <win.ps1> <unix.sh> -- [args...]");
  process.exit(1);
}

if (process.platform === "win32") {
  const ps1 = join(scriptDir, winScript);
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, ...args],
    { stdio: "inherit" },
  );
  process.exit(result.status ?? 1);
}

const sh = join(scriptDir, unixScript);
const result = spawnSync("bash", [sh, ...args], { stdio: "inherit" });
process.exit(result.status ?? 1);
