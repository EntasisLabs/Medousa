/** Paste/drop images into notes as inline `data:image/…;base64,…` markdown (no vault file). */

import { formatImageEmbedMarkdown } from "$lib/utils/vaultLocalImages";

/** Soft cap — larger pastes stay possible via manual attach; toast when over. */
export const VAULT_INLINE_IMAGE_MAX_BYTES = 2 * 1024 * 1024;

export type VaultImagePasteResult =
  | { ok: true; markdown: string; byteLength: number }
  | { ok: false; reason: "no-image" | "too-large" | "read-failed"; message: string };

function imageFilesFromDataTransfer(data: DataTransfer | null): File[] {
  if (!data) return [];
  const fromFiles = Array.from(data.files ?? []).filter((file) =>
    file.type.startsWith("image/"),
  );
  if (fromFiles.length > 0) return fromFiles;

  const items = Array.from(data.items ?? []);
  const fromItems: File[] = [];
  for (const item of items) {
    if (item.kind !== "file" || !item.type.startsWith("image/")) continue;
    const file = item.getAsFile();
    if (file) fromItems.push(file);
  }
  return fromItems;
}

/** Sync probe — use before preventDefault so text paste stays intact. */
export function dataTransferHasImage(data: DataTransfer | null): boolean {
  if (!data) return false;
  if (Array.from(data.files ?? []).some((file) => file.type.startsWith("image/"))) {
    return true;
  }
  return Array.from(data.items ?? []).some(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
}

async function readFileAsDataUrl(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  // Chunked btoa avoids call-stack issues on larger pastes (still under soft cap).
  const chunk = 0x8000;
  let base64 = "";
  for (let i = 0; i < bytes.length; i += chunk) {
    const slice = bytes.subarray(i, i + chunk);
    base64 += btoa(String.fromCharCode(...slice));
  }
  const mime = file.type.trim() || "image/png";
  return `data:${mime};base64,${base64}`;
}

export function altFromImageFile(file: File): string {
  const base = file.name.replace(/\.[^.]+$/, "").trim();
  return base || "image";
}

/** Build markdown for a data URL (used by tests + paste path). */
export function formatInlineImageMarkdown(dataUrl: string, alt?: string): string {
  return formatImageEmbedMarkdown(dataUrl, alt ?? "image");
}

export async function markdownFromImageFile(
  file: File,
  options?: { maxBytes?: number },
): Promise<VaultImagePasteResult> {
  const maxBytes = options?.maxBytes ?? VAULT_INLINE_IMAGE_MAX_BYTES;
  if (!file.type.startsWith("image/")) {
    return { ok: false, reason: "no-image", message: "No image on clipboard" };
  }
  if (file.size > maxBytes) {
    const mb = (maxBytes / (1024 * 1024)).toFixed(0);
    return {
      ok: false,
      reason: "too-large",
      message: `Image is too large to embed inline (max ${mb} MB). Use Linked files instead.`,
    };
  }

  try {
    const dataUrl = await readFileAsDataUrl(file);
    return {
      ok: true,
      markdown: formatInlineImageMarkdown(dataUrl, altFromImageFile(file)),
      byteLength: file.size,
    };
  } catch {
    return {
      ok: false,
      reason: "read-failed",
      message: "Could not read the pasted image",
    };
  }
}

/**
 * Extract the first image from a paste/drop DataTransfer and encode as markdown.
 * Returns `no-image` when the transfer has no image payload (caller should allow default paste).
 */
export async function markdownFromImageDataTransfer(
  data: DataTransfer | null,
  options?: { maxBytes?: number },
): Promise<VaultImagePasteResult> {
  const file = imageFilesFromDataTransfer(data)[0];
  if (!file) {
    return { ok: false, reason: "no-image", message: "No image on clipboard" };
  }
  return markdownFromImageFile(file, options);
}
