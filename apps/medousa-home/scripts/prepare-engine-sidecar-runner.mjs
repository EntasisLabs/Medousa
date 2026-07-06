#!/usr/bin/env node
/**
 * Cross-platform entry for prepare-engine-sidecar (bash on Unix, PowerShell on Windows).
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);

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
  const ps1 = join(scriptDir, "prepare-engine-sidecar.ps1");
  const psArgs = toPowerShellArgs(args);
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, ...psArgs],
    { stdio: "inherit" },
  );
  process.exit(result.status ?? 1);
}

const sh = join(scriptDir, "prepare-engine-sidecar.sh");
const result = spawnSync("bash", [sh, ...args], { stdio: "inherit" });
process.exit(result.status ?? 1);
