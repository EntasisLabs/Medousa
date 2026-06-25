export interface MediaRef {
  media_id: string;
  kind: string;
  mime: string;
  label?: string | null;
}

export interface MediaUploadResponse {
  media_id: string;
  mime: string;
  byte_size: number;
  label?: string | null;
  text_extracted?: boolean;
}

export interface ChatMediaAttachment {
  mediaId: string;
  kind: string;
  mime: string;
  label: string;
}

export function mediaKindFromMime(mime: string): string {
  const normalized = mime.trim().toLowerCase();
  if (normalized.startsWith("image/")) return "image";
  if (
    normalized.includes("spreadsheet") ||
    normalized.includes("excel") ||
    normalized === "text/csv" ||
    normalized === "text/tab-separated-values"
  ) {
    return "spreadsheet";
  }
  if (normalized.startsWith("audio/")) return "audio";
  return "document";
}

export function isImageMediaRef(ref: MediaRef): boolean {
  if (ref.kind === "image") return true;
  return ref.mime.trim().toLowerCase().startsWith("image/");
}

export function hasVisionMediaRefs(refs: MediaRef[]): boolean {
  return refs.some(isImageMediaRef);
}

export function mediaRefFromUpload(
  response: MediaUploadResponse,
  label?: string | null,
): MediaRef {
  return {
    media_id: response.media_id,
    kind: mediaKindFromMime(response.mime),
    mime: response.mime,
    label: label ?? response.label ?? null,
  };
}
