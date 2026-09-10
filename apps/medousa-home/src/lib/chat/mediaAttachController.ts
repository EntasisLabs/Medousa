/**
 * Pending composer media: picker, drop, and path upload.
 */

import { readClipboardImages } from "$lib/utils/chatImagePaste";
import type { MediaRef } from "$lib/types/media";
import {
  pickChatAttachmentFiles,
  type ChatAttachmentPickerSource,
  uploadChatFiles,
  uploadChatPaths,
} from "$lib/utils/chatMediaUpload";
import { friendlyUserError, MAX_MEDIA_REFS_PER_TURN } from "$lib/utils/normieErrors";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";

const uploads = new WeakMap<ChatStoreHost, object>();

export function clearPendingMedia(host: ChatStoreHost) {
  uploads.delete(host);
  host.pendingMediaUploading = false;
  host.pendingMediaRefs = [];
}

export function removePendingMedia(host: ChatStoreHost, mediaId: string) {
  host.pendingMediaRefs = host.pendingMediaRefs.filter((ref) => ref.media_id !== mediaId);
}

export async function attachFilesFromPicker(
  host: ChatStoreHost,
  source: ChatAttachmentPickerSource = "all",
) {
  await attachPendingMedia(host, async (slots, sessionId, isCurrent) => {
    const files = (await pickChatAttachmentFiles(source)).slice(0, slots);
    if (!isCurrent()) return [];
    return uploadChatFiles(sessionId, files, isCurrent);
  });
}

export async function attachClipboardImages(host: ChatStoreHost) {
  await attachPendingMedia(host, async (slots, sessionId, isCurrent) => {
    const files = await readClipboardImages(slots);
    if (!isCurrent()) return [];
    return uploadChatFiles(sessionId, files, isCurrent);
  });
}

export async function attachDroppedFiles(host: ChatStoreHost, files: File[]) {
  if (files.length === 0) return;
  await attachPendingMedia(host, (slots, sessionId, isCurrent) => {
    if (files.length > slots) {
      throw new Error(`Only ${slots} more attachment${slots === 1 ? "" : "s"} can be added to this message.`);
    }
    return uploadChatFiles(sessionId, files, isCurrent);
  });
}

export async function attachDroppedPaths(host: ChatStoreHost, paths: string[]) {
  if (paths.length === 0) return;
  await attachPendingMedia(host, (slots, sessionId, isCurrent) =>
    uploadChatPaths(sessionId, paths.slice(0, slots), isCurrent),
  );
}

async function attachPendingMedia(
  host: ChatStoreHost,
  load: (slots: number, sessionId: string, isCurrent: () => boolean) => Promise<MediaRef[]>,
) {
  if (host.pendingMediaUploading) {
    host.setError("An attachment is still uploading. Wait for it to finish, then paste or attach again.");
    return;
  }
  const slots = MAX_MEDIA_REFS_PER_TURN - host.pendingMediaRefs.length;
  if (slots <= 0) {
    host.setError(friendlyUserError(`too many attachments (max ${MAX_MEDIA_REFS_PER_TURN})`));
    return;
  }
  const sessionId = host.sessionId;
  const epoch = host.workshopEpoch;
  const operation = {};
  uploads.set(host, operation);
  const isCurrent = () => uploads.get(host) === operation &&
    host.sessionId === sessionId && host.workshopEpoch === epoch;
  host.pendingMediaUploading = true;
  try {
    const refs = await load(slots, sessionId, isCurrent);
    if (!isCurrent()) return;
    if (refs.length > 0) {
      host.pendingMediaRefs = [...host.pendingMediaRefs, ...refs];
      host.streamError = null;
    }
  } catch (err) {
    if (isCurrent()) host.setError(err instanceof Error ? err.message : String(err));
  } finally {
    if (uploads.get(host) === operation) {
      uploads.delete(host);
      if (host.workshopEpoch === epoch) host.pendingMediaUploading = false;
    }
  }
}
