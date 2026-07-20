import type { ChannelId } from "$lib/types/messaging";

/** Desktop shell sidebar ↔ MessagingPanel selection bridge. */
export class MessagingShellStore {
  selectedChannel = $state<ChannelId>("telegram");
  search = $state("");

  selectChannel(id: ChannelId) {
    this.selectedChannel = id;
  }
}

export const messagingShell = new MessagingShellStore();
