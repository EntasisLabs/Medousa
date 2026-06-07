<script lang="ts">
  import { messaging } from "$lib/stores/messaging.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import {
    formatNumberCsv,
    formatStringCsv,
    parseNumberCsv,
    parseStringCsv,
  } from "$lib/messaging";
  import {
    MESSAGING_CHANNELS,
    type ChannelId,
  } from "$lib/types/messaging";
  import {
    channelStatus,
    statusClass,
    statusLabel,
  } from "$lib/utils/channelStatus";
  import { Hash, Layers, Phone, Send } from "@lucide/svelte";
  import type { Component } from "svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, health, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let selectedChannel = $state<ChannelId>("telegram");

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

  const channelIcons: Record<ChannelId, Component> = {
    telegram: Send,
    discord: Hash,
    slack: Layers,
    whatsapp: Phone,
  };

  const daemonOk = $derived(health?.ok ?? false);

  const filteredChannels = $derived(
    MESSAGING_CHANNELS.filter((channel) => {
      const query = search.trim().toLowerCase();
      if (!query) return true;
      return [channel.name, channel.description, channel.id]
        .join(" ")
        .toLowerCase()
        .includes(query);
    }),
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

  function selectChannel(id: ChannelId) {
    selectedChannel = id;
  }

  async function saveSelected() {
    if (selectedChannel === "telegram") {
      await messaging.saveTelegram({
        allowedUserIds: parseNumberCsv(telegramAllowedUsers),
        heartbeatNudgesEnabled: telegramHeartbeat,
        heartbeatChatIds: parseNumberCsv(telegramHeartbeatChats),
        botToken: telegramToken,
        clearToken: telegramClearToken,
      });
      return;
    }
    if (selectedChannel === "discord") {
      await messaging.saveDiscord({
        commandPrefix: discordPrefix.trim() || "!",
        heartbeatNudgesEnabled: discordHeartbeat,
        heartbeatChannelIds: parseNumberCsv(discordHeartbeatChannels),
        botToken: discordToken,
        clearToken: discordClearToken,
      });
      return;
    }
    if (selectedChannel === "slack") {
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
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 class="text-base font-semibold text-surface-50">Messaging</h1>
          <p class="text-xs text-surface-300">
            Channels share the same product config as TUI and CLI
          </p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => messaging.refresh()}
        >
          Refresh
        </button>
      </div>

      {#if !mobile}
        <label class="mt-3 block">
          <span class="sr-only">Search channels</span>
          <input
            class="input w-full max-w-md text-sm"
            type="search"
            placeholder="Search channels…"
            bind:value={search}
          />
        </label>
      {/if}
    </header>
  {:else}
    <div class="flex items-center justify-end border-b border-surface-500/40 px-4 py-2">
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={() => messaging.refresh()}
      >
        Refresh
      </button>
    </div>
  {/if}

  {#if mobile}
    <div class="mobile-channel-strip shrink-0 overflow-x-auto border-b border-surface-500/40 px-3 py-2">
      <div class="flex gap-2">
        {#each filteredChannels as channel (channel.id)}
          {@const Icon = channelIcons[channel.id]}
          {@const status = channelStatus(channel.id, messaging.summary, daemonOk)}
          <button
            type="button"
            class="mobile-channel-chip {selectedChannel === channel.id
              ? 'mobile-channel-chip-active'
              : ''}"
            onclick={() => selectChannel(channel.id)}
          >
            <Icon size={14} strokeWidth={1.75} />
            <span>{channel.name}</span>
            <span class="text-[9px] uppercase {statusClass(status)}">
              {statusLabel(status)}
            </span>
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden {mobile ? 'flex-col' : ''}">
    {#if !mobile}
    <div class="w-[min(280px,34%)] shrink-0 overflow-y-auto border-r border-surface-500/40 px-3 py-3">
      {#if messaging.loading && !messaging.summary}
        <p class="workshop-muted">Loading channels…</p>
      {:else if messaging.error}
        <p class="text-sm text-warning-400">{messaging.error}</p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filteredChannels as channel (channel.id)}
            {@const Icon = channelIcons[channel.id]}
            {@const status = channelStatus(
              channel.id,
              messaging.summary,
              daemonOk,
            )}
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedChannel ===
                channel.id
                  ? 'bg-surface-800/80'
                  : ''}"
                onclick={() => selectChannel(channel.id)}
              >
                <span class="shrink-0 text-surface-400">
                  <Icon size={16} />
                </span>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <p class="truncate font-medium text-surface-100">
                      {channel.name}
                    </p>
                    <span class="text-[10px] uppercase tracking-wide {statusClass(status)}">
                      {statusLabel(status)}
                    </span>
                  </div>
                  <p class="workshop-faint mt-0.5 truncate text-[11px]">
                    {channel.description}
                  </p>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
    {/if}

    <div class="mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4">
      {#if selectedChannel === "telegram"}
        <h2 class="workshop-section-title">Telegram</h2>
        <p class="workshop-faint mt-2 text-sm">
          Bot token is stored in the system keychain when available, otherwise
          under <code class="markdown-inline-code">secrets/telegram_bot_token</code>.
        </p>

        <div class="mt-4 space-y-4">
          <label class="block">
            <span class="workshop-label">Bot token</span>
            {#if messaging.summary?.telegram.credentialsSet}
              <p class="mt-1 text-xs text-primary-300">Credentials set</p>
            {:else}
              <p class="mt-1 text-xs text-surface-500">Not configured</p>
            {/if}
            <input
              class="input mt-2 w-full max-w-xl font-mono text-[11px]"
              type="password"
              placeholder="Paste new token to replace"
              bind:value={telegramToken}
            />
            <label class="mt-2 flex items-center gap-2 text-xs text-surface-400">
              <input
                type="checkbox"
                class="checkbox"
                bind:checked={telegramClearToken}
              />
              Clear saved token on save
            </label>
          </label>

          <label class="block">
            <span class="workshop-label">Allowed user IDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              placeholder="123456789, 987654321"
              bind:value={telegramAllowedUsers}
            />
          </label>

          <label class="flex items-center gap-2 text-sm text-surface-300">
            <input
              type="checkbox"
              class="checkbox"
              bind:checked={telegramHeartbeat}
            />
            Heartbeat nudges enabled
          </label>

          <label class="block">
            <span class="workshop-label">Heartbeat chat IDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              placeholder="-1001234567890"
              bind:value={telegramHeartbeatChats}
            />
          </label>
        </div>
      {:else if selectedChannel === "discord"}
        <h2 class="workshop-section-title">Discord</h2>
        <div class="mt-4 space-y-4">
          <label class="block">
            <span class="workshop-label">Bot token</span>
            {#if messaging.summary?.discord.credentialsSet}
              <p class="mt-1 text-xs text-primary-300">Credentials set</p>
            {:else}
              <p class="mt-1 text-xs text-surface-500">Not configured</p>
            {/if}
            <input
              class="input mt-2 w-full max-w-xl font-mono text-[11px]"
              type="password"
              placeholder="Paste new token to replace"
              bind:value={discordToken}
            />
            <label class="mt-2 flex items-center gap-2 text-xs text-surface-400">
              <input type="checkbox" class="checkbox" bind:checked={discordClearToken} />
              Clear saved token on save
            </label>
          </label>

          <label class="block">
            <span class="workshop-label">Command prefix</span>
            <input
              class="input mt-1 w-24 font-mono text-sm"
              bind:value={discordPrefix}
            />
          </label>

          <label class="flex items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={discordHeartbeat} />
            Heartbeat nudges enabled
          </label>

          <label class="block">
            <span class="workshop-label">Heartbeat channel IDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={discordHeartbeatChannels}
            />
          </label>
        </div>
      {:else if selectedChannel === "slack"}
        <h2 class="workshop-section-title">Slack</h2>
        <div class="mt-4 space-y-4">
          <label class="block">
            <span class="workshop-label">Bot token (xoxb)</span>
            {#if messaging.summary?.slack.botTokenSet}
              <p class="mt-1 text-xs text-primary-300">Credentials set</p>
            {:else}
              <p class="mt-1 text-xs text-surface-500">Not configured</p>
            {/if}
            <input
              class="input mt-2 w-full max-w-xl font-mono text-[11px]"
              type="password"
              bind:value={slackBotToken}
              placeholder="Paste new bot token"
            />
            <label class="mt-2 flex items-center gap-2 text-xs text-surface-400">
              <input type="checkbox" class="checkbox" bind:checked={slackClearBotToken} />
              Clear bot token on save
            </label>
          </label>

          <label class="block">
            <span class="workshop-label">App token (xapp)</span>
            {#if messaging.summary?.slack.appTokenSet}
              <p class="mt-1 text-xs text-primary-300">Credentials set</p>
            {:else}
              <p class="mt-1 text-xs text-surface-500">Not configured</p>
            {/if}
            <input
              class="input mt-2 w-full max-w-xl font-mono text-[11px]"
              type="password"
              bind:value={slackAppToken}
              placeholder="Paste new app token"
            />
            <label class="mt-2 flex items-center gap-2 text-xs text-surface-400">
              <input type="checkbox" class="checkbox" bind:checked={slackClearAppToken} />
              Clear app token on save
            </label>
          </label>

          <label class="block">
            <span class="workshop-label">Allowed user IDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={slackAllowedUsers}
            />
          </label>

          <label class="flex items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={slackHeartbeat} />
            Heartbeat nudges enabled
          </label>

          <label class="block">
            <span class="workshop-label">Heartbeat channel IDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={slackHeartbeatChannels}
            />
          </label>
        </div>
      {:else}
        <h2 class="workshop-section-title">WhatsApp</h2>
        <p class="workshop-faint mt-2 text-sm">
          Bridge configuration — no bot token; uses deliver bind and session DB path.
        </p>
        <div class="mt-4 space-y-4">
          <label class="block">
            <span class="workshop-label">Deliver bind</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={whatsappDeliverBind}
            />
          </label>

          <label class="block">
            <span class="workshop-label">Deliver URL (optional)</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={whatsappDeliverUrl}
            />
          </label>

          <label class="block">
            <span class="workshop-label">Session DB path (optional)</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={whatsappSessionDb}
            />
          </label>

          <label class="block">
            <span class="workshop-label">Allowed user JIDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={whatsappAllowedUsers}
            />
          </label>

          <label class="flex items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={whatsappHeartbeat} />
            Heartbeat nudges enabled
          </label>

          <label class="block">
            <span class="workshop-label">Heartbeat chat JIDs</span>
            <input
              class="input mt-1 w-full max-w-xl font-mono text-[11px]"
              bind:value={whatsappHeartbeatJids}
            />
          </label>
        </div>
      {/if}

      <div class="mt-6 flex items-center gap-3">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={messaging.saving}
          onclick={() => void saveSelected()}
        >
          {messaging.saving ? "Saving…" : "Save"}
        </button>
        {#if messaging.saveMessage}
          <p
            class="text-xs {messaging.saveMessage === 'Saved'
              ? 'text-primary-300'
              : 'text-warning-400'}"
          >
            {messaging.saveMessage}
          </p>
        {/if}
      </div>

      {#if !daemonOk}
        <p class="workshop-faint mt-4 text-xs">
          Daemon offline — channel config is saved locally; adapters connect when
          services start.
        </p>
      {/if}
    </div>
  </div>
</section>
