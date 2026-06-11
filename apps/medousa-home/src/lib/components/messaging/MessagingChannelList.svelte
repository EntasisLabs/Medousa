<script lang="ts">
  import {
    channelIconClasses,
    MESSAGING_CHANNELS,
    type ChannelId,
    type ProductConfigSummary,
  } from "$lib/types/messaging";
  import {
    channelStatus,
    statusChipVariant,
    statusDotClass,
    statusLabel,
  } from "$lib/utils/channelStatus";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
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
</script>

<div class="min-h-0">
  {#if loading && !summary}
    <p class="workshop-muted px-2 py-4 text-sm">Loading channels…</p>
  {:else if error}
    <p class="px-2 py-4 text-sm text-warning-400">{error}</p>
  {:else if filteredChannels.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm">No channels match your search.</p>
  {:else}
    <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
      {#each filteredChannels as channel (channel.id)}
        {@const Icon = channelIcons[channel.id]}
        {@const status = channelStatus(channel.id, summary, daemonOk)}
        <li>
          <button
            type="button"
            class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selected ===
            channel.id
              ? 'workshop-list-row-active'
              : ''}"
            onclick={() => onSelect(channel.id)}
          >
            <span class={channelIconClasses(channel.id)} aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-2">
                <span class="truncate text-sm font-medium text-surface-100">
                  {channel.name}
                </span>
                <WorkshopLivelinessChip variant={statusChipVariant(status)} />
              </span>
              <span class="workshop-faint mt-0.5 block truncate text-[11px]">
                {channel.description}
              </span>
            </span>
            <span class={statusDotClass(status)} title={statusLabel(status)} aria-hidden="true"></span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
