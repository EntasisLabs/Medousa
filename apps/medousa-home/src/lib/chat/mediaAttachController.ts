/**
 * Pending composer media: picker, drop, and path upload.
 */

import type { MediaRef } from "$lib/types/media";
import {
  attachChatFiles,
  uploadChatFiles,
  uploadChatPaths,
} from "$lib/utils/chatMediaUpload";
import { friendlyUserError, MAX_MEDIA_REFS_PER_TURN } from "$lib/utils/normieErrors";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";

export function clearPendingMedia(host: ChatStoreHost) {
  host.pendingMediaRefs = [];
}

export function removePendingMedia(host: ChatStoreHost, mediaId: string) {
  host.pendingMediaRefs = host.pendingMediaRefs.filter((ref) => ref.media_id !== mediaId);
}

export async function attachFilesFromPicker(host: ChatStoreHost) {
  await attachPendingMedia(host, (slots) => attachChatFiles(host.sessionId, { maxNew: slots }));
}

export async function attachDroppedFiles(host: ChatStoreHost, files: File[]) {
  if (files.length === 0) return;
  await attachPendingMedia(host, (slots) =>
    uploadChatFiles(host.sessionId, files.slice(0, slots)),
  );
}

export async function attachDroppedPaths(host: ChatStoreHost, paths: string[]) {
  if (paths.length === 0) return;
  await attachPendingMedia(host, (slots) =>
    uploadChatPaths(host.sessionId, paths.slice(0, slots)),
  );
}

async function attachPendingMedia(
  host: ChatStoreHost,
  load: (slots: number) => Promise<MediaRef[]>,
) {
  if (host.pendingMediaUploading) return;
  const slots = MAX_MEDIA_REFS_PER_TURN - host.pendingMediaRefs.length;
  if (slots <= 0) {
    host.setError(friendlyUserError(`too many attachments (max ${MAX_MEDIA_REFS_PER_TURN})`));
    return;
  }
  host.pendingMediaUploading = true;
  try {
    const refs = await load(slots);
    if (refs.length > 0) {
      host.pendingMediaRefs = [...host.pendingMediaRefs, ...refs];
      host.streamError = null;
    }
  } catch (err) {
    host.setError(err instanceof Error ? err.message : String(err));
  } finally {
    host.pendingMediaUploading = false;
  }
}
