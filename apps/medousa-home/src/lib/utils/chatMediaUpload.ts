import {
  readMediaImagePath,
  uploadMediaBytes,
  uploadMediaPath,
} from "$lib/daemon";
import {
  mediaKindFromMime,
  mediaRefFromUpload,
  type MediaRef,
} from "$lib/types/media";
import {
  friendlyMediaUploadError,
  MAX_MEDIA_REFS_PER_TURN,
  MAX_MEDIA_UPLOAD_MB,
} from "$lib/utils/normieErrors";
import { guessMimeFromPath } from "$lib/utils/vaultAttachments";
import { nativePathNeedsImageNormalization } from "$lib/utils/chatImageFormats";

export type ChatAttachmentPickerSource = "all" | "photos" | "camera";

const IMAGE_ACCEPT =
  "image/*,.heic,.heif,.avif,.bmp,.tif,.tiff";
const FILE_ACCEPT =
  `${IMAGE_ACCEPT},.pdf,.csv,.tsv,.txt,.md,.xlsx,.xls,.docx`;
const MAX_MEDIA_UPLOAD_BYTES = MAX_MEDIA_UPLOAD_MB * 1024 * 1024;

async function normalizeUploadFile(file: File): Promise<File> {
  const { normalizeChatUploadFile } = await import("$lib/utils/chatImageNormalization");
  return normalizeChatUploadFile(file);
}

export async function pickChatAttachmentFiles(
  source: ChatAttachmentPickerSource = "all",
): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = source !== "camera";
    input.accept = source === "all" ? FILE_ACCEPT : IMAGE_ACCEPT;
    if (source === "camera") input.setAttribute("capture", "environment");
    input.style.display = "none";
    input.addEventListener("change", () => {
      const files = [...(input.files ?? [])];
      input.remove();
      resolve(files);
    });
    input.addEventListener("cancel", () => {
      input.remove();
      resolve([]);
    });
    document.body.appendChild(input);
    input.click();
  });
}

export async function uploadChatFiles(
  sessionId: string,
  files: File[],
): Promise<MediaRef[]> {
  const refs: MediaRef[] = [];
  for (const file of files) {
    try {
      if (file.size > MAX_MEDIA_UPLOAD_BYTES) {
        throw new Error("file exceeds max size");
      }
      const normalized = await normalizeUploadFile(file);
      const bytes = new Uint8Array(await normalized.arrayBuffer());
      const response = await uploadMediaBytes(
        sessionId,
        normalized.name,
        normalized.type || guessMimeFromPath(normalized.name),
        bytes,
        normalized.name,
      );
      refs.push(mediaRefFromUpload(response, normalized.name));
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      throw new Error(friendlyMediaUploadError(raw, file.name));
    }
  }
  return refs;
}

function fileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop()?.trim() || path;
}

/** Upload native desktop drops, whose Tauri event exposes paths rather than File objects. */
export async function uploadChatPaths(
  sessionId: string,
  paths: string[],
): Promise<MediaRef[]> {
  const refs: MediaRef[] = [];
  for (const path of paths) {
    const label = fileNameFromPath(path);
    if (nativePathNeedsImageNormalization(path)) {
      try {
        const payload = await readMediaImagePath(path);
        const file = new File([payload.bytes], payload.filename, {
          type: payload.mime,
        });
        refs.push(...(await uploadChatFiles(sessionId, [file])));
      } catch (err) {
        const raw = err instanceof Error ? err.message : String(err);
        throw new Error(friendlyMediaUploadError(raw, label));
      }
      continue;
    }
    try {
      const response = await uploadMediaPath(sessionId, path, label);
      refs.push(mediaRefFromUpload(response, label));
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      throw new Error(friendlyMediaUploadError(raw, label));
    }
  }
  return refs;
}

export async function attachChatFiles(
  sessionId: string,
  options?: { maxNew?: number; source?: ChatAttachmentPickerSource },
): Promise<MediaRef[]> {
  const maxNew = options?.maxNew ?? MAX_MEDIA_REFS_PER_TURN;
  if (maxNew <= 0) {
    throw new Error(
      friendlyMediaUploadError(
        `too many attachments (max ${MAX_MEDIA_REFS_PER_TURN})`,
      ),
    );
  }

  const files = (await pickChatAttachmentFiles(options?.source)).slice(0, maxNew);
  if (files.length === 0) return [];
  return uploadChatFiles(sessionId, files);
}

export function pendingMediaLabels(refs: MediaRef[]): string {
  return refs
    .map((ref) => ref.label?.trim() || ref.media_id)
    .join(", ");
}

export function chatMediaAttachmentsFromRefs(refs: MediaRef[]) {
  return refs.map((ref) => ({
    mediaId: ref.media_id,
    kind: ref.kind || mediaKindFromMime(ref.mime),
    mime: ref.mime,
    label: ref.label?.trim() || ref.media_id,
  }));
}
