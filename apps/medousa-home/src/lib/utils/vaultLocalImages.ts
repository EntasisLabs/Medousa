/** Phase G4 — resolve local / daemon markdown image paths for vault preview. */

import { getVaultFile } from "$lib/daemon";
import { isTauri } from "$lib/platform";
import {
  getVaultRootAbsolutePath,
  localFilePreviewUrl,
  resolveVaultNoteAbsolutePath,
} from "$lib/utils/vaultFilesystem";
import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";

const IMAGE_EXT =
  /\.(png|jpe?g|gif|webp|svg|bmp|ico|heic|heif|avif)$/i;

const daemonPreviewUrlCache = new Map<string, string>();

export function clearDaemonImagePreviewCache() {
  for (const url of daemonPreviewUrlCache.values()) {
    URL.revokeObjectURL(url);
  }
  daemonPreviewUrlCache.clear();
}

export function isRemoteImageHref(href: string): boolean {
  const trimmed = href.trim().toLowerCase();
  return (
    trimmed.startsWith("http://") ||
    trimmed.startsWith("https://") ||
    trimmed.startsWith("data:") ||
    trimmed.startsWith("blob:")
  );
}

export function isLocalImageHref(href: string): boolean {
  const trimmed = href.trim();
  if (!trimmed || isRemoteImageHref(trimmed)) return false;
  if (trimmed.startsWith("wikilink:")) return false;
  // Obsidian `path|400` / `path|400x240` — ignore size when detecting images.
  const withoutSize = trimmed.includes("|")
    ? trimmed.slice(0, trimmed.lastIndexOf("|")).trim()
    : trimmed;
  const pathOnly = withoutSize.split("?")[0]?.split("#")[0] ?? withoutSize;
  return IMAGE_EXT.test(pathOnly);
}

function joinPath(base: string, relative: string): string {
  const normalized = relative.replace(/\\/g, "/");
  if (/^[A-Za-z]:\//.test(normalized) || normalized.startsWith("/")) {
    return normalized;
  }

  const stack = base.replace(/\\/g, "/").split("/").filter(Boolean);
  for (const part of normalized.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") stack.pop();
    else stack.push(part);
  }
  return stack.join("/");
}

function isAbsoluteDiskPath(path: string): boolean {
  const normalized = path.replace(/\\/g, "/");
  return /^[A-Za-z]:\//.test(normalized) || normalized.startsWith("/");
}

/** Vault-relative path for daemon file fetch (null if absolute / unsafe). */
export function resolveVaultRelativeImagePath(
  rawPath: string,
  noteVaultPath: string | null,
): string | null {
  const trimmed = rawPath.trim().replace(/\\/g, "/");
  if (!trimmed || isRemoteImageHref(trimmed) || isAbsoluteDiskPath(trimmed)) {
    return null;
  }
  const cleaned = trimmed.replace(/^\.\//, "");
  if (cleaned.includes("..")) return null;

  if (!noteVaultPath?.includes("/")) {
    return cleaned;
  }
  const noteDir = noteVaultPath.slice(0, noteVaultPath.lastIndexOf("/"));
  return joinPath(noteDir, cleaned);
}

/** Resolve a markdown image path to an absolute file path on disk (co-located only). */
export async function resolveLocalImagePath(
  rawPath: string,
  noteVaultPath: string | null,
): Promise<string | null> {
  if (!isCoLocatedWorkshop()) return null;
  const trimmed = rawPath.trim();
  if (!trimmed || isRemoteImageHref(trimmed)) return null;

  const normalized = trimmed.replace(/\\/g, "/");
  if (isAbsoluteDiskPath(normalized)) {
    return normalized;
  }

  const vaultRoot = await getVaultRootAbsolutePath();
  if (!vaultRoot) return null;

  if (noteVaultPath) {
    const noteAbsolute = await resolveVaultNoteAbsolutePath(noteVaultPath);
    if (noteAbsolute) {
      const noteDir = noteAbsolute.includes("/")
        ? noteAbsolute.slice(0, noteAbsolute.lastIndexOf("/"))
        : vaultRoot;
      return joinPath(noteDir, normalized);
    }
  }

  return joinPath(vaultRoot, normalized.replace(/^\.\//, ""));
}

export async function embedPathForNote(
  filePath: string,
  noteVaultPath: string | null,
): Promise<string> {
  const normalized = filePath.replace(/\\/g, "/");
  if (isAbsoluteDiskPath(normalized)) {
    if (!isCoLocatedWorkshop()) return normalized;
    const noteAbsolute = noteVaultPath
      ? await resolveVaultNoteAbsolutePath(noteVaultPath)
      : null;
    const vaultRoot = await getVaultRootAbsolutePath();
    if (noteAbsolute && vaultRoot && normalized.startsWith(`${vaultRoot}/`)) {
      const noteDir = noteAbsolute.includes("/")
        ? noteAbsolute.slice(0, noteAbsolute.lastIndexOf("/"))
        : vaultRoot;
      if (normalized.startsWith(`${noteDir}/`)) {
        return normalized.slice(noteDir.length + 1);
      }
      return normalized.slice(vaultRoot.length + 1);
    }
    return normalized;
  }
  return normalized;
}

export function formatImageEmbedMarkdown(embedPath: string, alt?: string): string {
  const label = (alt ?? embedPath.split("/").pop() ?? "image").replace(/[\[\]]/g, "");
  // Angle-bracket destinations keep long data: URLs / spaces / () intact in CommonMark.
  const needsQuotes =
    /[()\s]/.test(embedPath) || embedPath.startsWith("data:") || embedPath.length > 200;
  const href = needsQuotes ? `<${embedPath}>` : embedPath;
  return `![${label}](${href})\n`;
}

async function previewUrlFromDaemonFile(vaultRelativePath: string): Promise<string | null> {
  if (!isTauri()) return null;
  const key = vaultRelativePath.trim();
  const cached = daemonPreviewUrlCache.get(key);
  if (cached) return cached;
  try {
    const file = await getVaultFile(key);
    const binary = Uint8Array.from(atob(file.base64), (char) => char.charCodeAt(0));
    const blob = new Blob([binary], {
      type: file.contentType || "application/octet-stream",
    });
    const url = URL.createObjectURL(blob);
    daemonPreviewUrlCache.set(key, url);
    return url;
  } catch {
    return null;
  }
}

export async function resolveLocalImagePreviewUrl(
  rawPath: string,
  noteVaultPath: string | null,
): Promise<string | null> {
  if (isCoLocatedWorkshop()) {
    const absolute = await resolveLocalImagePath(rawPath, noteVaultPath);
    if (absolute) {
      const local = await localFilePreviewUrl(absolute);
      if (local) return local;
    }
  }

  const relative = resolveVaultRelativeImagePath(rawPath, noteVaultPath);
  if (!relative) return null;
  return previewUrlFromDaemonFile(relative);
}

export { localFilePreviewUrl };
