import { uploadMediaBytes, uploadMediaPath } from "$lib/daemon";
import {
  mediaKindFromMime,
  mediaRefFromUpload,
  type MediaRef,
} from "$lib/types/media";
import {
  friendlyMediaUploadError,
  MAX_MEDIA_REFS_PER_TURN,
} from "$lib/utils/normieErrors";
import { guessMimeFromPath } from "$lib/utils/vaultAttachments";
import { isTauri } from "$lib/window";

function fileNameFromPath(path: string): string {
  return path.split("/").pop()?.split("\\").pop() ?? path;
}

export async function pickChatAttachmentFiles(): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.accept = "image/*,.pdf,.csv,.tsv,.txt,.md,.xlsx,.xls,.docx";
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
      const bytes = [...new Uint8Array(await file.arrayBuffer())];
      const response = await uploadMediaBytes(
        sessionId,
        file.name,
        file.type || guessMimeFromPath(file.name),
        bytes,
        file.name,
      );
      refs.push(mediaRefFromUpload(response, file.name));
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      throw new Error(friendlyMediaUploadError(raw, file.name));
    }
  }
  return refs;
}

export async function attachChatFiles(
  sessionId: string,
  options?: { maxNew?: number },
): Promise<MediaRef[]> {
  const maxNew = options?.maxNew ?? MAX_MEDIA_REFS_PER_TURN;
  if (maxNew <= 0) {
    throw new Error(
      friendlyMediaUploadError(
        `too many attachments (max ${MAX_MEDIA_REFS_PER_TURN})`,
      ),
    );
  }

  if (isTauri()) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: true,
      title: "Attach files",
    });
    if (!selected) return [];
    const paths = (Array.isArray(selected) ? selected : [selected]).slice(0, maxNew);
    const refs: MediaRef[] = [];
    for (const path of paths) {
      const name = fileNameFromPath(path);
      try {
        const response = await uploadMediaPath(sessionId, path, name);
        refs.push(mediaRefFromUpload(response, name));
      } catch (err) {
        const raw = err instanceof Error ? err.message : String(err);
        throw new Error(friendlyMediaUploadError(raw, name));
      }
    }
    return refs;
  }

  const files = (await pickChatAttachmentFiles()).slice(0, maxNew);
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
