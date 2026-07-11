<script lang="ts">
  import { MoreHorizontal, Paperclip } from "@lucide/svelte";
  import PeerAvatar from "$lib/components/peers/PeerAvatar.svelte";
  import type { PeerMessage, TrustedWorkshopSummary } from "$lib/utils/lanShareApi";
  import {
    formatPeerDateSeparator,
    formatPeerMessageTime,
    peerMessageDateKey,
  } from "$lib/utils/peerHomePreview";

  interface Props {
    peer: TrustedWorkshopSummary;
    messages: PeerMessage[];
    identityLabel: string;
    busy?: boolean;
    renaming?: boolean;
    renameDraft?: string;
    menuOpen?: boolean;
    onToggleMenu: () => void;
    onMarkRead: () => void;
    onStartRename: () => void;
    onCommitRename: () => void;
    onCancelRename: () => void;
    onRenameDraftChange: (value: string) => void;
    onRemove: () => void;
    onReconnect: () => void;
  }

  let {
    peer,
    messages,
    identityLabel,
    busy = false,
    renaming = false,
    renameDraft = "",
    menuOpen = false,
    onToggleMenu,
    onMarkRead,
    onStartRename,
    onCommitRename,
    onCancelRename,
    onRenameDraftChange,
    onRemove,
    onReconnect,
  }: Props = $props();

  let threadEl = $state<HTMLDivElement | null>(null);

  const ordered = $derived(
    [...messages].sort(
      (a, b) => Date.parse(a.sentAt) - Date.parse(b.sentAt) || a.id.localeCompare(b.id),
    ),
  );

  const statusKind = $derived.by(() => {
    if (peer.inbound) return "inbound" as const;
    if (!peer.hasSessionToken) return "reconnect" as const;
    return "ready" as const;
  });

  const statusLabel = $derived.by(() => {
    if (peer.inbound) return "Connected to you";
    if (!peer.hasSessionToken) return "Needs reconnect";
    return "Connected";
  });

  const avatarStatus = $derived.by(() => {
    if (peer.inbound) return "ready" as const;
    if (!peer.hasSessionToken) return "reconnect" as const;
    return "ready" as const;
  });

  function isOutbound(message: PeerMessage): boolean {
    return message.direction === "out";
  }

  $effect(() => {
    ordered.length;
    peer.workshopId;
    queueMicrotask(() => {
      if (!threadEl) return;
      threadEl.scrollTop = threadEl.scrollHeight;
    });
  });
</script>

<header class="peers-thread-head">
  <div class="peers-thread-identity">
    <PeerAvatar
      label={renaming ? renameDraft || peer.label : peer.label}
      size="lg"
      status={avatarStatus}
    />
    <div>
      {#if renaming}
        <input
          class="peers-rename-input"
          type="text"
          value={renameDraft}
          disabled={busy}
          aria-label="Peer display name"
          autofocus
          oninput={(event) => onRenameDraftChange((event.currentTarget as HTMLInputElement).value)}
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              onCommitRename();
            } else if (event.key === "Escape") {
              event.preventDefault();
              onCancelRename();
            }
          }}
          onblur={() => onCommitRename()}
        />
      {:else}
        <h2 class="peers-thread-name">{peer.label}</h2>
      {/if}
      <div class="peers-thread-status-row">
        <span class="peers-thread-status-dot peers-thread-status-dot--{statusKind}" aria-hidden="true"></span>
        <p class="peers-thread-status">{statusLabel}</p>
        {#if identityLabel}
          <span class="peers-identity-meta" aria-label={identityLabel}>· {identityLabel}</span>
        {/if}
        {#if !peer.inbound && !peer.hasSessionToken}
          <button type="button" class="peers-reconnect-btn" disabled={busy} onclick={onReconnect}>
            Reconnect
          </button>
        {/if}
      </div>
    </div>
  </div>
  <div class="peers-thread-menu-wrap">
    <button
      type="button"
      class="peers-icon-btn"
      aria-label="Peer options"
      aria-expanded={menuOpen}
      onclick={onToggleMenu}
    >
      <MoreHorizontal size={18} />
    </button>
    {#if menuOpen}
      <div class="peers-menu" role="menu">
        <button type="button" role="menuitem" class="peers-menu-item" disabled={busy} onclick={onMarkRead}>
          Mark read
        </button>
        {#if !peer.inbound}
          <button
            type="button"
            role="menuitem"
            class="peers-menu-item"
            disabled={busy}
            onclick={onStartRename}
          >
            Rename
          </button>
        {/if}
        <button
          type="button"
          role="menuitem"
          class="peers-menu-item peers-menu-danger"
          disabled={busy}
          onclick={onRemove}
        >
          Remove peer
        </button>
      </div>
    {/if}
  </div>
</header>

<div class="peers-thread" bind:this={threadEl}>
  {#if ordered.length === 0}
    <div class="peers-thread-empty">
      <p>Say hi to {peer.label.split(/\s+/)[0]}.</p>
    </div>
  {:else}
    {#each ordered as message, index (message.id)}
      {@const prev = ordered[index - 1]}
      {@const showDate =
        !prev || peerMessageDateKey(prev.sentAt) !== peerMessageDateKey(message.sentAt)}
      {#if showDate}
        <p class="peers-date-sep">{formatPeerDateSeparator(message.sentAt)}</p>
      {/if}
      <div
        class="peers-bubble"
        class:peers-bubble-out={isOutbound(message)}
        class:peers-bubble-unread={!isOutbound(message) && !message.readAt}
      >
        <p class="peers-bubble-body">{message.body}</p>
        {#if message.attachmentResult}
          <p class="peers-bubble-attach">
            <Paperclip size={10} strokeWidth={2} />
            {message.attachmentResult.summary ??
              (message.attachmentResult.imported ? "Attachment imported" : "Attachment")}
          </p>
        {/if}
        <span class="peers-bubble-time">
          {isOutbound(message) ? "You · " : ""}{formatPeerMessageTime(message.sentAt)}
        </span>
      </div>
    {/each}
  {/if}
</div>
