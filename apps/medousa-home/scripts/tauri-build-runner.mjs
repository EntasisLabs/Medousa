#!/usr/bin/env node
/**
 * Cross-platform entry for tauri build (PowerShell + MSVC env on Windows).
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const homeDir = join(scriptDir, "..");
const args = process.argv.slice(2);

if (process.platform === "win32") {
  const ps1 = join(homeDir, "..", "..", "scripts", "dev", "tauri-build.ps1");
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, "-HomeDir", homeDir, ...args],
    { stdio: "inherit" },
  );
  process.exit(result.status ?? 1);
}

const result = spawnSync("npx", ["tauri", "build", ...args], {
  cwd: homeDir,
  stdio: "inherit",
  shell: true,
});
process.exit(result.status ?? 1);
