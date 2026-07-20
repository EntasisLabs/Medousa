/** Paste/drop images into notes as inline `data:image/…;base64,…` markdown (no vault file). */

import { formatImageEmbedMarkdown } from "$lib/utils/vaultLocalImages";

/** Soft cap — larger pastes stay possible via manual attach; toast when over. */
export const VAULT_INLINE_IMAGE_MAX_BYTES = 2 * 1024 * 1024;

const WEB_SAFE_IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

export type VaultImagePasteResult =
  | { ok: true; markdown: string; dataUrl: string; alt: string; byteLength: number }
  | { ok: false; reason: "no-image" | "too-large" | "read-failed"; message: string };

/**
 * Grab image File(s) while the paste/drop event is still live.
 * Callers must invoke this synchronously in the event handler — DataTransfer
 * is often emptied once the handler returns (async read → empty/corrupt bytes).
 */
export function imageFilesFromDataTransfer(data: DataTransfer | null): File[] {
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

/** First image file, or null. Sync — see `imageFilesFromDataTransfer`. */
export function imageFileFromDataTransfer(data: DataTransfer | null): File | null {
  return imageFilesFromDataTransfer(data)[0] ?? null;
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

function bytesToBase64(bytes: Uint8Array): string {
  // Avoid `String.fromCharCode(...slice)` — large spreads blow JSC/WK arg limits.
  const chunk = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += chunk) {
    const slice = bytes.subarray(i, i + chunk);
    binary += String.fromCharCode.apply(null, slice as unknown as number[]);
  }
  return btoa(binary);
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result === "string" && result.startsWith("data:image/")) {
        resolve(result);
        return;
      }
      reject(new Error("Could not read image data"));
    };
    reader.onerror = () => reject(reader.error ?? new Error("Could not read image"));
    reader.readAsDataURL(file);
  });
}

async function readFileAsDataUrlFallback(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (bytes.byteLength === 0) {
    throw new Error("Empty image data");
  }
  const mime = file.type.trim() || "image/png";
  return `data:${mime};base64,${bytesToBase64(bytes)}`;
}

async function encodeAsPngDataUrl(file: File): Promise<string> {
  if (typeof createImageBitmap !== "function" || typeof document === "undefined") {
    throw new Error("Cannot convert image");
  }
  const bitmap = await createImageBitmap(file);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("No canvas context");
    ctx.drawImage(bitmap, 0, 0);
    return canvas.toDataURL("image/png");
  } finally {
    bitmap.close();
  }
}

async function encodeImageFile(file: File): Promise<string> {
  const mime = (file.type.trim() || "image/png").toLowerCase();
  if (!WEB_SAFE_IMAGE_TYPES.has(mime)) {
    try {
      return await encodeAsPngDataUrl(file);
    } catch {
      // Fall through — some hosts still render the original MIME.
    }
  }

  try {
    return await readFileAsDataUrl(file);
  } catch {
    return readFileAsDataUrlFallback(file);
  }
}

export function altFromImageFile(file: File): string {
  const base = file.name.replace(/\.[^.]+$/, "").trim();
  // Clipboard fakes often use "image.png" — keep a short stable alt.
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
  if (file.size <= 0) {
    return {
      ok: false,
      reason: "read-failed",
      message: "Could not read the pasted image",
    };
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
    const dataUrl = await encodeImageFile(file);
    // Guard against empty/truncated payloads that render as broken images.
    const comma = dataUrl.indexOf(",");
    const payload = comma >= 0 ? dataUrl.slice(comma + 1) : "";
    if (payload.length < 8) {
      return {
        ok: false,
        reason: "read-failed",
        message: "Could not read the pasted image",
      };
    }
    const alt = altFromImageFile(file);
    return {
      ok: true,
      markdown: formatInlineImageMarkdown(dataUrl, alt),
      dataUrl,
      alt,
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
 * Prefer sync `imageFileFromDataTransfer` + `markdownFromImageFile` in event handlers.
 */
export async function markdownFromImageDataTransfer(
  data: DataTransfer | null,
  options?: { maxBytes?: number },
): Promise<VaultImagePasteResult> {
  const file = imageFileFromDataTransfer(data);
  if (!file) {
    return { ok: false, reason: "no-image", message: "No image on clipboard" };
  }
  return markdownFromImageFile(file, options);
}
