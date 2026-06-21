<script lang="ts">
  import MessagingChannelDetail from "$lib/components/messaging/MessagingChannelDetail.svelte";
  import MessagingChannelList from "$lib/components/messaging/MessagingChannelList.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import {
    formatNumberCsv,
    formatStringCsv,
    parseNumberCsv,
    parseStringCsv,
  } from "$lib/messaging";
  import type { ChannelId } from "$lib/types/messaging";
  import { ChevronLeft } from "@lucide/svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, health, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let selectedChannel = $state<ChannelId>("telegram");
  let mobileDetailOpen = $state(false);

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
    messaging.saveMessage = null;
    if (mobile) {
      mobileDetailOpen = true;
    }
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

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (!mobileDetailOpen) return false;
      mobileDetailOpen = false;
      return true;
    });
  });
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="min-w-0">
          <h1 class="text-base font-semibold text-surface-50">Messaging</h1>
          <p class="workshop-header-line mt-1">
            Channels — who can reach the workshop and where she replies
          </p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface shrink-0"
          onclick={() => messaging.refresh()}
        >
          Refresh
        </button>
      </div>

      {#if !mobile || !mobileDetailOpen}
        <label class="mt-3 block">
          <span class="sr-only">Search messaging</span>
          <input
            class="input w-full max-w-lg text-sm"
            type="search"
            placeholder="Search messaging…"
            bind:value={search}
          />
        </label>
      {/if}
    </header>
  {:else}
    <div class="flex items-center justify-between border-b border-surface-500/40 px-4 py-2">
      {#if mobile && mobileDetailOpen}
        <button
          type="button"
          class="mobile-icon-btn"
          aria-label="Back to channels"
          onclick={() => {
            mobileDetailOpen = false;
          }}
        >
          <ChevronLeft size={20} strokeWidth={1.75} />
        </button>
      {:else}
        <p class="workshop-faint text-xs">Channels &amp; delivery</p>
      {/if}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={() => messaging.refresh()}
      >
        Refresh
      </button>
    </div>
    {#if mobile && !mobileDetailOpen}
      <div class="border-b border-surface-500/40 px-4 py-2">
        <input
          class="input w-full text-sm"
          type="search"
          placeholder="Search messaging…"
          bind:value={search}
        />
      </div>
    {/if}
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if !mobile || !mobileDetailOpen}
      <aside
        class="workshop-list-pane mobile-you-scroll min-w-0 shrink-0 overflow-y-auto px-3 py-3 {mobile
          ? 'w-full'
          : 'w-[min(300px,34%)] border-r border-surface-500/40'}"
      >
        <MessagingChannelList
          {search}
          selected={selectedChannel}
          summary={messaging.summary}
          {daemonOk}
          loading={messaging.loading}
          error={messaging.error}
          onSelect={selectChannel}
        />
      </aside>
    {/if}

    {#if !mobile || mobileDetailOpen}
      <div
        class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4 {mobile
          ? ''
          : 'border-l border-surface-500/40'}"
      >
        <MessagingChannelDetail
          channelId={selectedChannel}
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
    {/if}
  </div>
</section>
