<script lang="ts">
  import {
    channelIconClasses,
    MESSAGING_CHANNELS,
    type ChannelId,
    type ProductConfigSummary,
  } from "$lib/types/messaging";
  import {
    channelStatus,
    statusDotClass,
    statusLabel,
  } from "$lib/utils/channelStatus";
  import { Hash, Layers, Phone, Send } from "@lucide/svelte";
  import type { Component } from "svelte";

  interface Props {
    search: string;
    selected: ChannelId;
    summary: ProductConfigSummary | null;
    daemonOk: boolean;
    loading: boolean;
    error: string | null;
    onSelect: (id: ChannelId) => void;
  }

  let {
    search,
    selected,
    summary,
    daemonOk,
    loading,
    error,
    onSelect,
  }: Props = $props();

  const channelIcons: Record<ChannelId, Component> = {
    telegram: Send,
    discord: Hash,
    slack: Layers,
    whatsapp: Phone,
  };

  const filteredChannels = $derived(
    MESSAGING_CHANNELS.filter((channel) => {
      const query = search.trim().toLowerCase();
      if (!query) return true;
      return [channel.name, channel.description, channel.tagline, channel.id]
        .join(" ")
        .toLowerCase()
        .includes(query);
    }),
  );

  const readyCount = $derived(
    MESSAGING_CHANNELS.filter((channel) => {
      const status = channelStatus(channel.id, summary, daemonOk);
      return status === "connected" || status === "ready";
    }).length,
  );
</script>

<div class="msg-rail">
  {#if loading && !summary}
    <p class="workshop-muted px-1 py-4 text-sm">Loading channels…</p>
  {:else if error}
    <p class="px-1 py-4 text-sm text-warning-400">{error}</p>
  {:else if filteredChannels.length === 0}
    <p class="workshop-muted px-1 py-4 text-sm">No channels match.</p>
  {:else}
    {#if !search.trim()}
      <p class="msg-rail-lead">
        {readyCount} of {MESSAGING_CHANNELS.length} ready
      </p>
    {/if}
    <ul class="msg-rail-list">
      {#each filteredChannels as channel (channel.id)}
        {@const Icon = channelIcons[channel.id]}
        {@const status = channelStatus(channel.id, summary, daemonOk)}
        <li>
          <button
            type="button"
            class="msg-rail-row {selected === channel.id ? 'msg-rail-row-active' : ''}"
            onclick={() => onSelect(channel.id)}
          >
            <span class={channelIconClasses(channel.id)} aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="min-w-0 flex-1">
              <span class="msg-rail-name">{channel.name}</span>
              <span class="msg-rail-desc">{channel.description}</span>
            </span>
            <span class="msg-rail-status">
              <span class={statusDotClass(status)} title={statusLabel(status)} aria-hidden="true"
              ></span>
              <span class="msg-rail-status-label">{statusLabel(status)}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .msg-rail-lead {
    margin: 0 0 0.75rem;
    padding: 0 0.35rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-rail-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .msg-rail-row {
    display: flex;
    width: 100%;
    align-items: flex-start;
    gap: 0.75rem;
    margin: 0;
    padding: 0.7rem 0.55rem;
    border: none;
    border-radius: 0.65rem;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .msg-rail-row:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.45);
  }

  .msg-rail-row-active {
    background: rgb(var(--color-primary-500) / 0.09);
  }

  .msg-rail-name {
    display: block;
    font-size: 0.875rem;
    font-weight: 550;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .msg-rail-desc {
    display: block;
    margin-top: 0.15rem;
    font-size: 0.71875rem;
    line-height: 1.35;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-rail-status {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.35rem;
    padding-top: 0.2rem;
  }

  .msg-rail-status-label {
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-rail-row-active .msg-rail-status-label {
    color: rgb(var(--color-primary-300));
  }

  :global(.msg-rail-status .messaging-status-dot) {
    margin-top: 0;
  }
</style>
