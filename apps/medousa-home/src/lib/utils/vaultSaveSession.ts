/** Session id for vault saves when a note is pinned to workshop chat. */

import { chat } from "$lib/stores/chat.svelte";

export function workshopSessionIdForVaultSave(path: string | null): string | undefined {
  if (!path) return undefined;
  if (!chat.pinVaultNoteContext || !chat.vaultNoteContext) return undefined;
  if (chat.vaultNoteContext.path !== path) return undefined;
  const sessionId = chat.sessionId?.trim();
  return sessionId || undefined;
}
