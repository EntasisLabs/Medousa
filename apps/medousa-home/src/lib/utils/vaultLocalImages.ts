/** Phase G4 — resolve local markdown image paths for vault preview. */

import {
  getVaultRootAbsolutePath,
  localFilePreviewUrl,
  resolveVaultNoteAbsolutePath,
} from "$lib/utils/vaultFilesystem";

const IMAGE_EXT =
  /\.(png|jpe?g|gif|webp|svg|bmp|ico|heic|heif|avif)$/i;

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
  return IMAGE_EXT.test(trimmed.split("?")[0]?.split("#")[0] ?? trimmed);
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

/** Resolve a markdown image path to an absolute file path on disk. */
export async function resolveLocalImagePath(
  rawPath: string,
  noteVaultPath: string | null,
): Promise<string | null> {
  const trimmed = rawPath.trim();
  if (!trimmed || isRemoteImageHref(trimmed)) return null;

  const normalized = trimmed.replace(/\\/g, "/");
  if (/^[A-Za-z]:\//.test(normalized) || normalized.startsWith("/")) {
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
      const besideNote = joinPath(noteDir, normalized);
      return besideNote;
    }
  }

  return joinPath(vaultRoot, normalized.replace(/^\.\//, ""));
}

export async function embedPathForNote(
  filePath: string,
  noteVaultPath: string | null,
): Promise<string> {
  const normalized = filePath.replace(/\\/g, "/");
  if (/^[A-Za-z]:\//.test(normalized) || normalized.startsWith("/")) {
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
  const needsQuotes = /[()\s]/.test(embedPath);
  const href = needsQuotes ? `<${embedPath}>` : embedPath;
  return `![${label}](${href})\n`;
}

export async function resolveLocalImagePreviewUrl(
  rawPath: string,
  noteVaultPath: string | null,
): Promise<string | null> {
  const absolute = await resolveLocalImagePath(rawPath, noteVaultPath);
  if (!absolute) return null;
  return localFilePreviewUrl(absolute);
}

export { localFilePreviewUrl };
