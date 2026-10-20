import { untrack } from "svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { AddressedConversation } from "./addressedConversation.svelte";
import { createConversationSurface } from "./conversationSurface.svelte";

/** Spotlight visibility owns preview refreshes, never the lifetime of a running turn. */
export function createSpotlightConversation(picking: () => boolean, onWorkshopChange: () => void) {
  const conversation = createConversationSurface(() => commandSpotlight.open, picking, onWorkshopChange);
  $effect(() => {
    if (conversation.target && (commandSpotlight.mode !== "default" || commandSpotlight.promptStep || commandSpotlight.activityFilter)) {
      untrack(() => { conversation.clear(); onWorkshopChange(); });
    }
  });
  return conversation;
}

export function openAddressedConversation(conversation: AddressedConversation) {
  if (!conversation.sessionId) return;
  shellTabs.openChat(conversation.sessionId, { activate: true });
  commandSpotlight.closeSpotlight();
}
