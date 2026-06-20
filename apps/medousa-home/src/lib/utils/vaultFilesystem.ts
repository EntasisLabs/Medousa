/** Resolve vault note paths on disk (desktop Tauri). */

import { getMedousaConfigPaths } from "$lib/config";
import { isTauri } from "$lib/window";

export async function resolveVaultNoteAbsolutePath(
  vaultRelativePath: string,
): Promise<string | null> {
  const relative = vaultRelativePath.trim().replace(/^\/+/, "");
  if (!relative) return null;
  try {
    const root = await getVaultRootAbsolutePath();
    if (!root) return null;
    return `${root}/${relative}`;
  } catch {
    return null;
  }
}

export async function getVaultRootAbsolutePath(): Promise<string | null> {
  try {
    const paths = await getMedousaConfigPaths();
    return `${paths.dataDir.replace(/\/+$/, "")}/vault`;
  } catch {
    return null;
  }
}

export async function localFilePreviewUrl(absolutePath: string): Promise<string | null> {
  if (!isTauri() || !absolutePath.trim()) return null;
  try {
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(absolutePath.replace(/\\/g, "/"));
  } catch {
    return null;
  }
}

export async function revealVaultNoteInFinder(vaultRelativePath: string): Promise<void> {
  if (!isTauri()) return;
  const absolute = await resolveVaultNoteAbsolutePath(vaultRelativePath);
  if (!absolute) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(absolute);
}

export async function openVaultNoteWithDefaultApp(
  vaultRelativePath: string,
): Promise<void> {
  if (!isTauri()) return;
  const absolute = await resolveVaultNoteAbsolutePath(vaultRelativePath);
  if (!absolute) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(absolute);
}

export async function revealFileInFinder(absolutePath: string): Promise<void> {
  if (!isTauri() || !absolutePath.trim()) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(absolutePath);
}

export async function openFileWithDefaultApp(absolutePath: string): Promise<void> {
  if (!absolutePath.trim()) return;
  if (absolutePath.startsWith("http://") || absolutePath.startsWith("https://")) {
    window.open(absolutePath, "_blank", "noopener,noreferrer");
    return;
  }
  if (!isTauri()) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(absolutePath);
}
