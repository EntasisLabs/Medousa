<script lang="ts">
  import MessagingChannelDetail from "$lib/components/messaging/MessagingChannelDetail.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import {
    formatNumberCsv,
    formatStringCsv,
    parseNumberCsv,
    parseStringCsv,
  } from "$lib/messaging";
  import {
    channelIconClasses,
    MESSAGING_CHANNELS,
    type ChannelId,
  } from "$lib/types/messaging";
  import {
    channelStatus,
    statusDotClass,
    statusLabel,
  } from "$lib/utils/channelStatus";
  import { Hash, Layers, Phone, Send, X } from "@lucide/svelte";
  import type { Component } from "svelte";

  interface Props {
    visible?: boolean;
    health: DaemonHealth | null;
  }

  let { visible = true, health }: Props = $props();

  let sheetChannel = $state<ChannelId | null>(null);

  let telegramAllowedUsers = $state("");
  let telegramHeartbeatChats = $state("");
  let telegramHeartbeat = $state(false);
  let telegramToken = $state("");
  let telegramClearToken = $state(false);

  let discordPrefix = $state("!");
  let discordHeartbeatChannels = $state("");
  let discordHeartbeat = $state(false);
  let discordToken = $state("");
  let discordClearToken = $state(false);

  let slackAllowedUsers = $state("");
  let slackHeartbeatChannels = $state("");
  let slackHeartbeat = $state(false);
  let slackBotToken = $state("");
  let slackAppToken = $state("");
  let slackClearBotToken = $state(false);
  let slackClearAppToken = $state(false);

  let whatsappDeliverBind = $state("127.0.0.1:7422");
  let whatsappDeliverUrl = $state("");
  let whatsappSessionDb = $state("");
  let whatsappAllowedUsers = $state("");
  let whatsappHeartbeatJids = $state("");
  let whatsappHeartbeat = $state(false);

  const daemonOk = $derived(health?.ok ?? false);
  const channelIcons: Record<ChannelId, Component> = {
    telegram: Send,
    discord: Hash,
    slack: Layers,
    whatsapp: Phone,
  };

  const readyCount = $derived(
    MESSAGING_CHANNELS.filter((channel) => {
      const status = channelStatus(channel.id, messaging.summary, daemonOk);
      return status === "connected" || status === "ready";
    }).length,
  );

  const sheetMeta = $derived(
    sheetChannel
      ? MESSAGING_CHANNELS.find((channel) => channel.id === sheetChannel) ?? null
      : null,
  );

  $effect(() => {
    if (!visible) return;
    void messaging.refresh();
  });

  $effect(() => {
    const summary = messaging.summary;
    if (!summary) return;

    telegramAllowedUsers = formatNumberCsv(summary.telegram.allowedUserIds);
    telegramHeartbeatChats = formatNumberCsv(summary.telegram.heartbeatChatIds);
    telegramHeartbeat = summary.telegram.heartbeatNudgesEnabled;
    telegramToken = "";
    telegramClearToken = false;

    discordPrefix = summary.discord.commandPrefix || "!";
    discordHeartbeatChannels = formatNumberCsv(summary.discord.heartbeatChannelIds);
    discordHeartbeat = summary.discord.heartbeatNudgesEnabled;
    discordToken = "";
    discordClearToken = false;

    slackAllowedUsers = formatStringCsv(summary.slack.allowedUserIds);
    slackHeartbeatChannels = formatStringCsv(summary.slack.heartbeatChannelIds);
    slackHeartbeat = summary.slack.heartbeatNudgesEnabled;
    slackBotToken = "";
    slackAppToken = "";
    slackClearBotToken = false;
    slackClearAppToken = false;

    whatsappDeliverBind = summary.whatsapp.deliverBind || "127.0.0.1:7422";
    whatsappDeliverUrl = summary.whatsapp.deliverUrl ?? "";
    whatsappSessionDb = summary.whatsapp.sessionDbPath ?? "";
    whatsappAllowedUsers = formatStringCsv(summary.whatsapp.allowedUserIds);
    whatsappHeartbeatJids = formatStringCsv(summary.whatsapp.heartbeatChatJids);
    whatsappHeartbeat = summary.whatsapp.heartbeatNudgesEnabled;
  });

  function openChannel(id: ChannelId) {
    sheetChannel = id;
    messaging.saveMessage = null;
  }

  function closeSheet() {
    sheetChannel = null;
    messaging.saveMessage = null;
  }

  async function saveSelected() {
    if (!sheetChannel) return;
    if (sheetChannel === "telegram") {
      await messaging.saveTelegram({
        allowedUserIds: parseNumberCsv(telegramAllowedUsers),
        heartbeatNudgesEnabled: telegramHeartbeat,
        heartbeatChatIds: parseNumberCsv(telegramHeartbeatChats),
        botToken: telegramToken,
        clearToken: telegramClearToken,
      });
      return;
    }
    if (sheetChannel === "discord") {
      await messaging.saveDiscord({
        commandPrefix: discordPrefix.trim() || "!",
        heartbeatNudgesEnabled: discordHeartbeat,
        heartbeatChannelIds: parseNumberCsv(discordHeartbeatChannels),
        botToken: discordToken,
        clearToken: discordClearToken,
      });
      return;
    }
    if (sheetChannel === "slack") {
      await messaging.saveSlack({
        allowedUserIds: parseStringCsv(slackAllowedUsers),
        heartbeatNudgesEnabled: slackHeartbeat,
        heartbeatChannelIds: parseStringCsv(slackHeartbeatChannels),
        botToken: slackBotToken,
        appToken: slackAppToken,
        clearBotToken: slackClearBotToken,
        clearAppToken: slackClearAppToken,
      });
      return;
    }
    await messaging.saveWhatsApp({
      deliverBind: whatsappDeliverBind.trim() || "127.0.0.1:7422",
      deliverUrl: whatsappDeliverUrl.trim() || null,
      sessionDbPath: whatsappSessionDb.trim() || null,
      allowedUserIds: parseStringCsv(whatsappAllowedUsers),
      heartbeatNudgesEnabled: whatsappHeartbeat,
      heartbeatChatJids: parseStringCsv(whatsappHeartbeatJids),
    });
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSheet();
    }
  }
</script>

<div class="msg-calm">
  <div class="msg-calm-head">
    <p class="msg-calm-lead">{readyCount} of {MESSAGING_CHANNELS.length} ready</p>
    <button
      type="button"
      class="btn btn-xs variant-ghost-surface"
      onclick={() => void messaging.refresh()}
    >
      Refresh
    </button>
  </div>

  {#if messaging.loading && !messaging.summary}
    <p class="workshop-faint text-sm">Loading channels…</p>
  {:else if messaging.error}
    <p class="text-sm text-warning-400">{messaging.error}</p>
  {:else}
    <ul class="msg-calm-list">
      {#each MESSAGING_CHANNELS as channel (channel.id)}
        {@const Icon = channelIcons[channel.id]}
        {@const status = channelStatus(channel.id, messaging.summary, daemonOk)}
        <li>
          <button type="button" class="msg-calm-row" onclick={() => openChannel(channel.id)}>
            <span class={channelIconClasses(channel.id)} aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="msg-calm-copy">
              <span class="msg-calm-name">{channel.name}</span>
              <span class="msg-calm-desc">{channel.description}</span>
            </span>
            <span class="msg-calm-status">
              <span class={statusDotClass(status)} title={statusLabel(status)} aria-hidden="true"
              ></span>
              <span class="msg-calm-status-label">{statusLabel(status)}</span>
              <span class="msg-calm-action">Open</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if sheetChannel && sheetMeta}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="msg-sheet-backdrop"
    role="presentation"
    onclick={closeSheet}
    onkeydown={onSheetKeydown}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="msg-sheet"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="{sheetMeta.name} channel"
      onclick={(event) => event.stopPropagation()}
    >
      <header class="msg-sheet-header">
        <div class="min-w-0">
          <h3 class="msg-sheet-title">{sheetMeta.name}</h3>
          <p class="msg-sheet-meta">{sheetMeta.tagline}</p>
        </div>
        <button
          type="button"
          class="msg-sheet-close"
          aria-label="Close"
          onclick={closeSheet}
        >
          <X size={18} />
        </button>
      </header>
      <div class="msg-sheet-body">
        <MessagingChannelDetail
          channelId={sheetChannel}
          summary={messaging.summary}
          {daemonOk}
          saving={messaging.saving}
          saveMessage={messaging.saveMessage}
          onSave={saveSelected}
          bind:telegramAllowedUsers
          bind:telegramHeartbeatChats
          bind:telegramHeartbeat
          bind:telegramToken
          bind:telegramClearToken
          bind:discordPrefix
          bind:discordHeartbeatChannels
          bind:discordHeartbeat
          bind:discordToken
          bind:discordClearToken
          bind:slackAllowedUsers
          bind:slackHeartbeatChannels
          bind:slackHeartbeat
          bind:slackBotToken
          bind:slackAppToken
          bind:slackClearBotToken
          bind:slackClearAppToken
          bind:whatsappDeliverBind
          bind:whatsappDeliverUrl
          bind:whatsappSessionDb
          bind:whatsappAllowedUsers
          bind:whatsappHeartbeatJids
          bind:whatsappHeartbeat
        />
      </div>
    </div>
  </div>
{/if}

<style>
  .msg-calm-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.55rem;
  }

  .msg-calm-lead {
    margin: 0;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-500));
  }

  .msg-calm-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.5rem;
  }

  .msg-calm-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.75rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .msg-calm-row:hover {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .msg-calm-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .msg-calm-name {
    font-size: 0.85rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .msg-calm-desc {
    font-size: 0.7rem;
    line-height: 1.35;
    color: rgb(var(--color-surface-500));
  }

  .msg-calm-status {
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.2rem;
  }

  .msg-calm-status-label {
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .msg-calm-action {
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
  }

  .msg-sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.25rem;
    background: rgb(0 0 0 / 0.55);
  }

  .msg-sheet {
    display: flex;
    width: min(36rem, 100%);
    max-height: min(86vh, 48rem);
    flex-direction: column;
    overflow: hidden;
    border-radius: 0.85rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 18px 48px rgb(0 0 0 / 0.45);
  }

  .msg-sheet-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.9rem 1rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.28);
  }

  .msg-sheet-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .msg-sheet-meta {
    margin: 0.2rem 0 0;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-500));
  }

  .msg-sheet-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .msg-sheet-close:hover {
    background: rgb(var(--color-surface-800) / 0.7);
  }

  .msg-sheet-body {
    min-height: 0;
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 0.85rem 1rem 1.1rem;
  }
</style>
