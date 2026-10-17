import type { PendingAskCompletion } from "$lib/types/askJob";

/** Shell intent only: keep task data and rendering out of the eager AppShell graph. */
export class TaskOverlayState {
  newAskOpen = $state(false);
  askHistorySessionId = $state<string | null>(null);
  selectedCardId = $state<string | null>(null);
  pendingAskCompletion = $state<PendingAskCompletion | null>(null);
}

export const taskOverlays = new TaskOverlayState();
