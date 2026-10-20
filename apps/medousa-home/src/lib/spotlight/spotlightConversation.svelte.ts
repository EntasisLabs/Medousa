import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { untrack } from "svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { AddressedConversation } from "./addressedConversation.svelte";

/** Spotlight visibility owns preview refreshes, never the lifetime of a running turn. */
export function createSpotlightConversation(picking: () => boolean, onWorkshopChange: () => void) {
  const conversation = new AddressedConversation();
  const wantsTargets = $derived(picking());
  $effect(() => {
    if (!commandSpotlight.open || !wantsTargets) return;
    void chat.workshopScopeId;
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
    if (!commandSpotlight.open || !conversation.sessionId) return;
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible" && !conversation.sending) void conversation.refresh();
    }, 5000);
    return () => window.clearInterval(timer);
  });
  $effect(() => {
    if (conversation.target && (commandSpotlight.mode !== "default" || commandSpotlight.promptStep || commandSpotlight.activityFilter)) {
      untrack(() => { conversation.clear(); onWorkshopChange(); });
    }
  });
  $effect(() => { conversation.releaseInactive(); });
  return conversation;
}

export function openAddressedConversation(conversation: AddressedConversation) {
  if (!conversation.sessionId) return;
  shellTabs.openChat(conversation.sessionId, { activate: true });
  commandSpotlight.closeSpotlight();
}
