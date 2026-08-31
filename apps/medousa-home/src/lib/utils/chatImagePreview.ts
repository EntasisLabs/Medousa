const WEB_SAFE_IMAGE_MIMES = new Set([
  "image/jpeg",
  "image/png",
  "image/gif",
  "image/webp",
]);

export async function chatImageObjectUrl(
  bytes: Uint8Array,
  mime: string,
  label = "attachment",
): Promise<string> {
  const normalizedMime = mime.trim().toLowerCase();
  const file = new File([bytes], label, { type: normalizedMime });
  if (WEB_SAFE_IMAGE_MIMES.has(normalizedMime)) {
    return URL.createObjectURL(file);
  }
  const { normalizeChatUploadFile } = await import("$lib/utils/chatImageNormalization");
  return URL.createObjectURL(await normalizeChatUploadFile(file));
}
