<script lang="ts">
  import {
    channelCredentialsInsetClass,
    channelIconClasses,
    channelMeta,
    type ChannelId,
    type ProductConfigSummary,
  } from "$lib/types/messaging";
  import {
    channelStatus,
    statusChipVariant,
    statusLabel,
  } from "$lib/utils/channelStatus";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import { Hash, Layers, Phone, Send } from "@lucide/svelte";
  import type { Component } from "svelte";

  interface Props {
    channelId: ChannelId;
    summary: ProductConfigSummary | null;
    daemonOk: boolean;
    saving: boolean;
    saveMessage: string | null;
    onSave: () => void | Promise<void>;
    telegramAllowedUsers?: string;
    telegramHeartbeatChats?: string;
    telegramHeartbeat?: boolean;
    telegramToken?: string;
    telegramClearToken?: boolean;
    discordPrefix?: string;
    discordHeartbeatChannels?: string;
    discordHeartbeat?: boolean;
    discordToken?: string;
    discordClearToken?: boolean;
    slackAllowedUsers?: string;
    slackHeartbeatChannels?: string;
    slackHeartbeat?: boolean;
    slackBotToken?: string;
    slackAppToken?: string;
    slackClearBotToken?: boolean;
    slackClearAppToken?: boolean;
    whatsappDeliverBind?: string;
    whatsappDeliverUrl?: string;
    whatsappSessionDb?: string;
    whatsappAllowedUsers?: string;
    whatsappHeartbeatJids?: string;
    whatsappHeartbeat?: boolean;
  }

  let {
    channelId,
    summary,
    daemonOk,
    saving,
    saveMessage,
    onSave,
    telegramAllowedUsers = $bindable(""),
    telegramHeartbeatChats = $bindable(""),
    telegramHeartbeat = $bindable(false),
    telegramToken = $bindable(""),
    telegramClearToken = $bindable(false),
    discordPrefix = $bindable("!"),
    discordHeartbeatChannels = $bindable(""),
    discordHeartbeat = $bindable(false),
    discordToken = $bindable(""),
    discordClearToken = $bindable(false),
    slackAllowedUsers = $bindable(""),
    slackHeartbeatChannels = $bindable(""),
    slackHeartbeat = $bindable(false),
    slackBotToken = $bindable(""),
    slackAppToken = $bindable(""),
    slackClearBotToken = $bindable(false),
    slackClearAppToken = $bindable(false),
    whatsappDeliverBind = $bindable("127.0.0.1:7422"),
    whatsappDeliverUrl = $bindable(""),
    whatsappSessionDb = $bindable(""),
    whatsappAllowedUsers = $bindable(""),
    whatsappHeartbeatJids = $bindable(""),
    whatsappHeartbeat = $bindable(false),
  }: Props = $props();

  let advancedOpen = $state(false);

  const meta = $derived(channelMeta(channelId));
  const status = $derived(channelStatus(channelId, summary, daemonOk));

  const channelIcons: Record<ChannelId, Component> = {
    telegram: Send,
    discord: Hash,
    slack: Layers,
    whatsapp: Phone,
  };

  const Icon = $derived(channelIcons[channelId]);

  const advancedCount = $derived(
    channelId === "discord" || channelId === "telegram" || channelId === "slack"
      ? 2
      : channelId === "whatsapp"
        ? 2
        : 0,
  );
</script>

<article class="max-w-xl">
  <header class="flex items-start gap-3">
    <span class={channelIconClasses(channelId, true)} aria-hidden="true">
      <Icon size={20} strokeWidth={1.85} />
    </span>
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold tracking-tight text-surface-50">{meta.name}</h2>
      <p class="workshop-faint mt-1 text-sm leading-relaxed">{meta.tagline}</p>
      <div class="mt-3 flex flex-wrap gap-2">
        <WorkshopLivelinessChip variant={statusChipVariant(status)} label={statusLabel(status)} />
        {#if !daemonOk}
          <WorkshopLivelinessChip variant="muted" label="Workshop offline" />
        {/if}
      </div>
    </div>
  </header>

  <section class="{channelCredentialsInsetClass(channelId)} mt-6">
    <h3 class="workshop-label">{meta.credentialsTitle}</h3>
    <p class="workshop-faint mt-2 text-sm leading-relaxed">{meta.credentialsBlurb}</p>
    {#if meta.setupGuideUrl}
      <a
        class="messaging-guide-link"
        href={meta.setupGuideUrl}
        target="_blank"
        rel="noopener noreferrer"
      >
        {meta.setupGuideLabel ?? "Open setup guide"} ↗
      </a>
    {/if}
  </section>

  <section class="mt-6">
    <h3 class="workshop-label">Required</h3>
    <div class="mt-3 space-y-4">
      {#if channelId === "telegram"}
        <label class="block">
          <span class="workshop-label">Bot token</span>
          {#if summary?.telegram.credentialsSet}
            <span class="mt-1 block text-xs text-primary-300">Stored securely</span>
          {:else}
            <span class="workshop-faint mt-1 block text-xs">Not configured</span>
          {/if}
          <input
            class="input mt-2 w-full font-mono text-xs"
            type="password"
            placeholder="123456:ABC-DEF…"
            bind:value={telegramToken}
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={telegramClearToken} />
            Clear saved token on save
          </label>
        </label>
      {:else if channelId === "discord"}
        <label class="block">
          <span class="workshop-label">Bot token</span>
          {#if summary?.discord.credentialsSet}
            <span class="mt-1 block text-xs text-primary-300">Stored securely</span>
          {:else}
            <span class="workshop-faint mt-1 block text-xs">Not configured</span>
          {/if}
          <input
            class="input mt-2 w-full font-mono text-xs"
            type="password"
            placeholder="Paste bot token"
            bind:value={discordToken}
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={discordClearToken} />
            Clear saved token on save
          </label>
        </label>
      {:else if channelId === "slack"}
        <label class="block">
          <span class="workshop-label">Bot token (xoxb)</span>
          {#if summary?.slack.botTokenSet}
            <span class="mt-1 block text-xs text-primary-300">Stored securely</span>
          {:else}
            <span class="workshop-faint mt-1 block text-xs">Not configured</span>
          {/if}
          <input
            class="input mt-2 w-full font-mono text-xs"
            type="password"
            placeholder="xoxb-…"
            bind:value={slackBotToken}
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={slackClearBotToken} />
            Clear bot token on save
          </label>
        </label>
        <label class="block">
          <span class="workshop-label">App token (xapp)</span>
          {#if summary?.slack.appTokenSet}
            <span class="mt-1 block text-xs text-primary-300">Stored securely</span>
          {:else}
            <span class="workshop-faint mt-1 block text-xs">Not configured</span>
          {/if}
          <input
            class="input mt-2 w-full font-mono text-xs"
            type="password"
            placeholder="xapp-…"
            bind:value={slackAppToken}
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={slackClearAppToken} />
            Clear app token on save
          </label>
        </label>
      {:else}
        <label class="block">
          <span class="workshop-label">Deliver bind</span>
          <span class="workshop-faint mt-0.5 block text-xs">Address the bridge listens on</span>
          <input class="input mt-2 w-full font-mono text-xs" bind:value={whatsappDeliverBind} />
        </label>
        <label class="block">
          <span class="workshop-label">Allowed user JIDs</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Comma-separated — who may message Medousa
          </span>
          <input
            class="input mt-2 w-full font-mono text-xs"
            placeholder="user@s.whatsapp.net"
            bind:value={whatsappAllowedUsers}
          />
        </label>
      {/if}
    </div>
  </section>

  <section class="mt-6">
    <h3 class="workshop-label">Recommended</h3>
    <div class="mt-3 space-y-4">
      {#if channelId === "telegram"}
        <label class="block">
          <span class="workshop-label">Allowed Telegram user IDs</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Comma-separated numeric IDs — required for ingest
          </span>
          <input
            class="input mt-2 w-full font-mono text-xs"
            placeholder="123456789, 987654321"
            bind:value={telegramAllowedUsers}
          />
        </label>
      {:else if channelId === "discord"}
        <label class="block">
          <span class="workshop-label">Command prefix</span>
          <span class="workshop-faint mt-0.5 block text-xs">Character before commands in Discord</span>
          <input class="input mt-2 w-24 font-mono text-sm" bind:value={discordPrefix} />
        </label>
      {:else if channelId === "slack"}
        <label class="block">
          <span class="workshop-label">Allowed Slack user IDs</span>
          <span class="workshop-faint mt-0.5 block text-xs">Comma-separated — required for ingest</span>
          <input class="input mt-2 w-full font-mono text-xs" bind:value={slackAllowedUsers} />
        </label>
      {:else}
        <label class="block">
          <span class="workshop-label">Deliver URL</span>
          <span class="workshop-faint mt-0.5 block text-xs">Optional override for bridge delivery</span>
          <input class="input mt-2 w-full font-mono text-xs" bind:value={whatsappDeliverUrl} />
        </label>
        <label class="block">
          <span class="workshop-label">Session DB path</span>
          <span class="workshop-faint mt-0.5 block text-xs">Optional path to bridge session database</span>
          <input class="input mt-2 w-full font-mono text-xs" bind:value={whatsappSessionDb} />
        </label>
      {/if}
    </div>
  </section>

  <section class="mt-6 border-t border-surface-500/30 pt-4">
    <button
      type="button"
      class="flex w-full items-center justify-between text-left text-sm font-medium text-surface-100"
      onclick={() => (advancedOpen = !advancedOpen)}
    >
      <span class="workshop-label">Advanced{#if advancedCount > 0}&nbsp;({advancedCount}){/if}</span>
      <span class="workshop-faint">{advancedOpen ? "▾" : "▸"}</span>
    </button>

    {#if advancedOpen}
      <div class="mt-4 space-y-4">
        {#if channelId === "telegram"}
          <label class="flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={telegramHeartbeat} />
            Heartbeat nudges enabled
          </label>
          <label class="block">
            <span class="workshop-label">Heartbeat chat IDs</span>
            <input
              class="input mt-2 w-full font-mono text-xs"
              placeholder="-1001234567890"
              bind:value={telegramHeartbeatChats}
            />
          </label>
        {:else if channelId === "discord"}
          <label class="flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={discordHeartbeat} />
            Heartbeat nudges enabled
          </label>
          <label class="block">
            <span class="workshop-label">Heartbeat channel IDs</span>
            <input
              class="input mt-2 w-full font-mono text-xs"
              bind:value={discordHeartbeatChannels}
            />
          </label>
        {:else if channelId === "slack"}
          <label class="flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={slackHeartbeat} />
            Heartbeat nudges enabled
          </label>
          <label class="block">
            <span class="workshop-label">Heartbeat channel IDs</span>
            <input
              class="input mt-2 w-full font-mono text-xs"
              bind:value={slackHeartbeatChannels}
            />
          </label>
        {:else}
          <label class="flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={whatsappHeartbeat} />
            Heartbeat nudges enabled
          </label>
          <label class="block">
            <span class="workshop-label">Heartbeat chat JIDs</span>
            <input
              class="input mt-2 w-full font-mono text-xs"
              bind:value={whatsappHeartbeatJids}
            />
          </label>
        {/if}
      </div>
    {/if}
  </section>

  <footer class="mt-8 flex flex-col gap-3 border-t border-surface-500/30 pt-6">
    <button
      type="button"
      class="btn variant-filled-primary w-fit"
      disabled={saving}
      onclick={() => void onSave()}
    >
      {saving ? "Saving…" : "Save channel"}
    </button>
    {#if saveMessage}
      <p class="text-xs {saveMessage === 'Saved' ? 'text-primary-300' : 'text-warning-400'}">
        {saveMessage}
      </p>
    {/if}
    {#if !daemonOk}
      <p class="workshop-faint text-xs">
        Workshop offline — config saves locally; adapters connect when the daemon starts.
      </p>
    {/if}
  </footer>
</article>
