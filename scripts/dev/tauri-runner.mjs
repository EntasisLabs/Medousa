#!/usr/bin/env node
/**
 * Cross-platform wrapper for the Tauri CLI.
 * On Windows: loads VS 2022 MSVC env + Medousa cargo cache via tauri-build.ps1.
 *
 * Run from an app package directory, e.g.:
 *   cd apps/medousa-installer && node ../../scripts/dev/tauri-runner.mjs build
 *   cd apps/medousa-home && node ../../scripts/dev/tauri-runner.mjs dev
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = process.cwd();
const args = process.argv.slice(2);

if (process.platform === "win32") {
  const ps1 = join(scriptDir, "tauri-build.ps1");
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, "-AppDir", appDir, ...args],
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

const tauriArgs = args.length > 0 ? args : ["build"];
const result = spawnSync("npx", ["tauri", ...tauriArgs], {
  cwd: appDir,
  stdio: "inherit",
  shell: true,
  env,
});
process.exit(result.status ?? 1);
