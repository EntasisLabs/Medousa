#!/usr/bin/env node
/**
 * Run a release script by basename on Unix (bash) or Windows (PowerShell).
 *
 * Usage: node scripts/release/release-runner.mjs build.ps1 build.sh -- [args...]
 *
 * On Windows, long-form flags like --target x86_64-pc-windows-msvc are converted
 * to PowerShell switches (-Target x86_64-pc-windows-msvc).
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

function toPascalCase(kebab) {
  return kebab
    .split("-")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

function toPowerShellArgs(unixArgs) {
  const out = [];
  for (let i = 0; i < unixArgs.length; i += 1) {
    const arg = unixArgs[i];
    if (arg === "--") continue;
    if (arg.startsWith("--")) {
      const key = arg.slice(2);
      const next = unixArgs[i + 1];
      const switchName = toPascalCase(key);
      if (next && !next.startsWith("-")) {
        out.push(`-${switchName}`, next);
        i += 1;
      } else {
        out.push(`-${switchName}`);
      }
      continue;
    }
    out.push(arg);
  }
  return out;
}

if (process.platform === "win32") {
  const ps1 = join(scriptDir, winScript);
  const psArgs = toPowerShellArgs(args);
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, ...psArgs],
    { stdio: "inherit" },
  );
  process.exit(result.status ?? 1);
}

const sh = join(scriptDir, unixScript);
const result = spawnSync("bash", [sh, ...args], { stdio: "inherit" });
process.exit(result.status ?? 1);
