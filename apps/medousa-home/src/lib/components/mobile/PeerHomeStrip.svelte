<script lang="ts">
  import { haptic } from "$lib/haptics";
  import { layout } from "$lib/stores/layout.svelte";
  import { setPendingPeerNavigation } from "$lib/peerNavigation";
  import {
    formatPeerRelativeTime,
    peerThreadPreviewLine,
    type PeerThreadPreview,
  } from "$lib/utils/peerHomePreview";

  interface Props {
    threads: PeerThreadPreview[];
  }

  let { threads }: Props = $props();

  function monogram(label: string): string {
    const parts = label.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return "?";
    if (parts.length === 1) return parts[0]!.slice(0, 1).toUpperCase();
    return `${parts[0]!.slice(0, 1)}${parts[1]!.slice(0, 1)}`.toUpperCase();
  }

  function openPeers() {
    haptic("light");
    layout.openMore("peers");
  }

  function openThread(workshopId: string) {
    haptic("light");
    setPendingPeerNavigation(workshopId);
    layout.openMore("peers");
  }
</script>

{#if threads.length > 0}
  <section class="mobile-home-peer-strip" aria-label="Recent peer messages">
    <div class="mobile-home-peer-strip-head">
      <p class="mobile-home-peer-strip-title">Messages</p>
      <button type="button" class="mobile-home-peer-strip-all" onclick={openPeers}>
        See all
      </button>
    </div>
    <ul class="mobile-home-peer-list">
      {#each threads as thread (thread.workshopId)}
        <li>
          <button
            type="button"
            class="mobile-home-peer-row"
            onclick={() => openThread(thread.workshopId)}
          >
            <span class="mobile-home-peer-avatar" aria-hidden="true">
              {monogram(thread.label)}
            </span>
            <span class="mobile-home-peer-copy">
              <span class="mobile-home-peer-top">
                <span class="mobile-home-peer-name">{thread.label}</span>
                {#if thread.lastMessage}
                  <span class="mobile-home-peer-time">
                    {formatPeerRelativeTime(thread.lastMessage.sentAt)}
                  </span>
                {/if}
              </span>
              <span class="mobile-home-peer-preview">
                {peerThreadPreviewLine(thread)}
              </span>
            </span>
            {#if thread.unreadCount > 0}
              <span class="mobile-home-peer-unread" aria-label="{thread.unreadCount} unread">
                {thread.unreadCount}
              </span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}
