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

const env = { ...process.env };
if (process.platform === "linux") {
  // linuxdeploy ships an old strip that chokes on .relr.dyn (SHT_RELR) in modern
  // distro libs (Arch, Fedora, etc.). Ubuntu 22.04 CI is unaffected but this is harmless there.
  env.NO_STRIP ??= "1";
  // AppImage bundlers are AppImages themselves; extract-and-run avoids needing FUSE.
  env.APPIMAGE_EXTRACT_AND_RUN ??= "1";
}

const result = spawnSync("npx", ["tauri", "build", ...args], {
  cwd: homeDir,
  stdio: "inherit",
  shell: true,
  env,
});
process.exit(result.status ?? 1);
