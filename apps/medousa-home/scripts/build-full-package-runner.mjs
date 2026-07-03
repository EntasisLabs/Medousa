#!/usr/bin/env node
/**
 * Cross-platform entry for build-full-package (PowerShell on Windows, bash elsewhere).
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);

if (process.platform === "win32") {
  const ps1 = join(scriptDir, "build-full-package.ps1");
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, ...args],
    { stdio: "inherit" },
  );
  process.exit(result.status ?? 1);
}

const sh = join(scriptDir, "build-full-package.sh");
const result = spawnSync("bash", [sh, ...args], { stdio: "inherit" });
process.exit(result.status ?? 1);
