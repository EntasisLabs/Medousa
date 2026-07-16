/** Resolve vault note paths on disk (desktop Tauri) — only when co-located. */

import { invoke } from "@tauri-apps/api/core";
import { listVaultRoots } from "$lib/daemon";
import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
import { isTauri } from "$lib/window";
import { readExternalFile } from "$lib/utils/externalDeskApi";

let cachedVaultRoot: { path: string; fetchedAt: number } | null = null;
const CACHE_MS = 5_000;

export async function resolveVaultNoteAbsolutePath(
  vaultRelativePath: string,
): Promise<string | null> {
  if (!isCoLocatedWorkshop()) return null;
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
  if (!isTauri() || !isCoLocatedWorkshop()) return null;
  const now = Date.now();
  if (cachedVaultRoot && now - cachedVaultRoot.fetchedAt < CACHE_MS) {
    return cachedVaultRoot.path;
  }
  try {
    const response = await listVaultRoots();
    const active =
      response.roots.find((root) => root.id === response.activeRootId) ??
      response.roots.find((root) => root.active) ??
      response.roots[0];
    const path = active?.path?.replace(/\/+$/, "") ?? null;
    if (path) {
      cachedVaultRoot = { path, fetchedAt: now };
    }
    return path;
  } catch {
    return null;
  }
}

export function invalidateVaultRootCache() {
  cachedVaultRoot = null;
}

export async function localFilePreviewUrl(absolutePath: string): Promise<string | null> {
  if (!isTauri() || !isCoLocatedWorkshop() || !absolutePath.trim()) return null;
  try {
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(absolutePath.replace(/\\/g, "/"));
  } catch {
    return null;
  }
}

export async function revealVaultNoteInFinder(vaultRelativePath: string): Promise<void> {
  if (!isTauri() || !isCoLocatedWorkshop()) return;
  const absolute = await resolveVaultNoteAbsolutePath(vaultRelativePath);
  if (!absolute) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(absolute);
}

export async function openVaultNoteWithDefaultApp(
  vaultRelativePath: string,
): Promise<void> {
  if (!isTauri() || !isCoLocatedWorkshop()) return;
  const absolute = await resolveVaultNoteAbsolutePath(vaultRelativePath);
  if (!absolute) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(absolute);
}

export async function revealFileInFinder(absolutePath: string): Promise<void> {
  if (!isTauri() || !isCoLocatedWorkshop() || !absolutePath.trim()) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(absolutePath);
}

export async function openFileWithDefaultApp(absolutePath: string): Promise<void> {
  if (!absolutePath.trim()) return;
  if (absolutePath.startsWith("http://") || absolutePath.startsWith("https://")) {
    window.open(absolutePath, "_blank", "noopener,noreferrer");
    return;
  }
  if (!isTauri() || !isCoLocatedWorkshop()) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(absolutePath);
}

/** Whether Finder / open-with-default are available for vault notes on this connection. */
export function canUseLocalVaultFilesystem(): boolean {
  return isTauri() && isCoLocatedWorkshop();
}

/** Pick a single markdown file without registering a vault root. */
export async function pickMarkdownFile(): Promise<string | null> {
  if (!isTauri() || !isCoLocatedWorkshop()) return null;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      title: "Open markdown file",
      filters: [{ name: "Markdown", extensions: ["md", "markdown"] }],
    });
    if (!selected) return null;
    return Array.isArray(selected) ? selected[0] ?? null : selected;
  } catch {
    return null;
  }
}

export async function readAbsoluteTextFile(absolutePath: string): Promise<string> {
  const payload = await readExternalFile(absolutePath);
  if (payload.kind !== "text") {
    throw new Error("Expected a text markdown file.");
  }
  return payload.content;
}

export async function writeAbsoluteTextFile(
  absolutePath: string,
  content: string,
): Promise<void> {
  if (!isTauri()) {
    throw new Error("Saving a loose file needs the Medousa desktop app.");
  }
  const bytes = Array.from(new TextEncoder().encode(content));
  await invoke("write_file_bytes", { path: absolutePath, bytes });
}

export function fileNameFromAbsolutePath(absolutePath: string): string {
  return absolutePath.split(/[/\\]/).pop() ?? absolutePath;
}
