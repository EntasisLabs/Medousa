<script lang="ts">
  import PeerAvatar from "$lib/components/peers/PeerAvatar.svelte";
  import type { PeerConversationRow } from "$lib/utils/peerHomePreview";
  import {
    formatPeerRelativeTime,
    peerThreadPreviewLine,
  } from "$lib/utils/peerHomePreview";

  interface Props {
    row: PeerConversationRow;
    selected?: boolean;
    onSelect: () => void;
  }

  let { row, selected = false, onSelect }: Props = $props();

  const when = $derived(
    row.lastMessage ? formatPeerRelativeTime(row.lastMessage.sentAt) : "",
  );
  const preview = $derived(peerThreadPreviewLine(row));
  const avatarStatus = $derived.by(() => {
    if (row.peer.inbound) return "ready" as const;
    if (!row.peer.hasSessionToken) return "reconnect" as const;
    if (row.nearby) return "nearby" as const;
    return "ready" as const;
  });
  const chip = $derived.by(() => {
    if (row.peer.inbound) return { kind: "inbound" as const, label: "Connected to you" };
    if (!row.peer.hasSessionToken) {
      return { kind: "reconnect" as const, label: "Needs reconnect" };
    }
    return null;
  });
</script>

<button
  type="button"
  class="peers-person"
  class:peers-person-active={selected}
  onclick={onSelect}
>
  <PeerAvatar label={row.label} status={avatarStatus} />
  <span class="peers-person-copy">
    <span class="peers-person-top">
      <span class="peers-person-name">{row.label}</span>
      {#if when}
        <span class="peers-person-when">{when}</span>
      {/if}
    </span>
    <span
      class="peers-person-preview"
      class:peers-person-preview-unread={row.unreadCount > 0}
    >
      {preview}
    </span>
    {#if chip}
      <span class="peers-person-chip peers-person-chip--{chip.kind}">{chip.label}</span>
    {/if}
  </span>
  {#if row.unreadCount > 0}
    <span class="peers-person-unread">{row.unreadCount}</span>
  {/if}
</button>
