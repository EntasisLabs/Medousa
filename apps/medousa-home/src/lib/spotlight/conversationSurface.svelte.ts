import { untrack } from "svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { chat } from "$lib/stores/chat.svelte";
import { AddressedConversation } from "./addressedConversation.svelte";

/** Visibility owns previews; admitted work continues after a surface closes. */
export function createConversationSurface(visible: () => boolean, picking: () => boolean, onWorkshopChange: () => void) {
  const conversation = new AddressedConversation();
  const wantsTargets = $derived(picking());
  $effect(() => {
    if (!visible() || !wantsTargets) return;
    void activeWorkshopId();
    untrack(() => { void conversation.loadRuntimes(); void chat.refreshSessions(); });
  });
  $effect(() => {
    void activeWorkshopId();
    void chat.sessionId;
    const scope = chat.workshopScopeId;
    if (conversation.target && conversation.target.scope !== scope) {
      untrack(() => { conversation.clear(); onWorkshopChange(); });
    }
  });
  $effect(() => {
    if (!visible() || !conversation.sessionId) return;
    untrack(() => { void conversation.refresh(); });
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible" && !conversation.sending) void conversation.refresh();
    }, 5000);
    return () => window.clearInterval(timer);
  });
  $effect(() => { conversation.releaseInactive(); });
  return conversation;
}
