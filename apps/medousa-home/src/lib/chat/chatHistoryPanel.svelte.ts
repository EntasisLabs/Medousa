import type { ChatHistoryController } from "./chatHistoryController.svelte";
import { untrack } from "svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";

let nextViewId = 0;

/** Tie retained history payloads to mounted, visible conversation surfaces. */
export function useChatHistoryPanel(history: ChatHistoryController, sessionId: () => string, visible: () => boolean) {
  const viewId = `chat-view-${nextViewId++}`;
  const scope = $derived(activeWorkshopId());
  $effect(() => {
    void scope;
    const id = sessionId();
    if (!visible() || !id) return;
    return untrack(() => history.retain(id, viewId));
  });
  return {
    messages: () => history.messagesFor(sessionId(), viewId),
    olderCursor: () => history.olderCursorFor(sessionId(), viewId),
    canLoadNewer: () => history.canLoadNewer(sessionId(), viewId),
    loading: () => history.loadingFor(sessionId(), viewId),
    older: () => history.loadOlder(sessionId(), viewId),
    newer: () => history.loadNewer(sessionId(), viewId),
    latest: () => history.latest(sessionId(), viewId),
  };
}
