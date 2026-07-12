<script lang="ts">
  import {
    channelIconClasses,
    channelMeta,
    type ChannelId,
    type ProductConfigSummary,
  } from "$lib/types/messaging";
  import { channelStatus, statusLabel } from "$lib/utils/channelStatus";
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

  $effect(() => {
    channelId;
    advancedOpen = false;
  });

  function secretHint(configured: boolean | undefined): string {
    return configured ? "Kept on this Mac — paste again only to replace" : "Waiting for a token";
  }
</script>

<article class="msg-story">
  <header class="msg-story-hero">
    <span class={channelIconClasses(channelId, true)} aria-hidden="true">
      <Icon size={20} strokeWidth={1.85} />
    </span>
    <div class="min-w-0 flex-1">
      <p class="msg-story-kicker">
        {statusLabel(status)}
        {#if !daemonOk}
          · engine offline
        {/if}
      </p>
      <h2 class="msg-story-title">{meta.name}</h2>
      <p class="msg-story-lead">{meta.tagline}</p>
    </div>
  </header>

  <section class="msg-story-chapter">
    <p class="msg-story-step">01</p>
    <div class="msg-story-chapter-body">
      <h3 class="msg-story-chapter-title">Connect</h3>
      <p class="msg-story-chapter-copy">
        {meta.credentialsBlurb}
        {#if meta.setupGuideUrl}
          <a
            class="msg-story-link"
            href={meta.setupGuideUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            {meta.setupGuideLabel ?? "Setup guide"} ↗
          </a>
        {/if}
      </p>

      {#if channelId === "telegram"}
        <label class="msg-field">
          <span class="msg-field-label">Bot token</span>
          <span class="msg-field-hint">{secretHint(summary?.telegram.credentialsSet)}</span>
          <input
            class="msg-field-input msg-field-mono"
            type="password"
            placeholder="123456:ABC-DEF…"
            bind:value={telegramToken}
          />
          <label class="msg-field-check">
            <input type="checkbox" class="checkbox" bind:checked={telegramClearToken} />
            Clear saved token on save
          </label>
        </label>
      {:else if channelId === "discord"}
        <label class="msg-field">
          <span class="msg-field-label">Bot token</span>
          <span class="msg-field-hint">{secretHint(summary?.discord.credentialsSet)}</span>
          <input
            class="msg-field-input msg-field-mono"
            type="password"
            placeholder="Paste the bot token"
            bind:value={discordToken}
          />
          <label class="msg-field-check">
            <input type="checkbox" class="checkbox" bind:checked={discordClearToken} />
            Clear saved token on save
          </label>
        </label>
      {:else if channelId === "slack"}
        <label class="msg-field">
          <span class="msg-field-label">Bot token</span>
          <span class="msg-field-hint">xoxb · {secretHint(summary?.slack.botTokenSet).toLowerCase()}</span>
          <input
            class="msg-field-input msg-field-mono"
            type="password"
            placeholder="xoxb-…"
            bind:value={slackBotToken}
          />
          <label class="msg-field-check">
            <input type="checkbox" class="checkbox" bind:checked={slackClearBotToken} />
            Clear on save
          </label>
        </label>
        <label class="msg-field">
          <span class="msg-field-label">App token</span>
          <span class="msg-field-hint">xapp · {secretHint(summary?.slack.appTokenSet).toLowerCase()}</span>
          <input
            class="msg-field-input msg-field-mono"
            type="password"
            placeholder="xapp-…"
            bind:value={slackAppToken}
          />
          <label class="msg-field-check">
            <input type="checkbox" class="checkbox" bind:checked={slackClearAppToken} />
            Clear on save
          </label>
        </label>
      {:else}
        <label class="msg-field">
          <span class="msg-field-label">Deliver bind</span>
          <span class="msg-field-hint">Where the bridge listens</span>
          <input class="msg-field-input msg-field-mono" bind:value={whatsappDeliverBind} />
        </label>
        <label class="msg-field">
          <span class="msg-field-label">Allowed JIDs</span>
          <span class="msg-field-hint">Who may message her — comma-separated</span>
          <input
            class="msg-field-input msg-field-mono"
            placeholder="user@s.whatsapp.net"
            bind:value={whatsappAllowedUsers}
          />
        </label>
      {/if}
    </div>
  </section>

  <section class="msg-story-chapter">
    <p class="msg-story-step">02</p>
    <div class="msg-story-chapter-body">
      <h3 class="msg-story-chapter-title">
        {#if channelId === "discord"}
          How she’s called
        {:else if channelId === "whatsapp"}
          Bridge extras
        {:else}
          Who may reach her
        {/if}
      </h3>
      <p class="msg-story-chapter-copy">
        {#if channelId === "telegram"}
          After the token is live, message the bot
          <span class="msg-story-mono">/whoami</span>
          and paste your numeric ID — only you get in.
        {:else if channelId === "discord"}
          The character before commands in your servers.
        {:else if channelId === "slack"}
          Socket Mode only listens to the Slack users you list here.
        {:else}
          Optional overrides — most bridges never need these.
        {/if}
      </p>

      {#if channelId === "telegram"}
        <label class="msg-field">
          <span class="msg-field-label">Your Telegram user ID</span>
          <span class="msg-field-hint">From /whoami</span>
          <input
            class="msg-field-input msg-field-mono"
            placeholder="123456789"
            bind:value={telegramAllowedUsers}
          />
        </label>
      {:else if channelId === "discord"}
        <label class="msg-field msg-field-narrow">
          <span class="msg-field-label">Command prefix</span>
          <input class="msg-field-input msg-field-mono" bind:value={discordPrefix} />
        </label>
      {:else if channelId === "slack"}
        <label class="msg-field">
          <span class="msg-field-label">Allowed Slack user IDs</span>
          <span class="msg-field-hint">Comma-separated</span>
          <input class="msg-field-input msg-field-mono" bind:value={slackAllowedUsers} />
        </label>
      {:else}
        <label class="msg-field">
          <span class="msg-field-label">Deliver URL</span>
          <span class="msg-field-hint">Optional</span>
          <input class="msg-field-input msg-field-mono" bind:value={whatsappDeliverUrl} />
        </label>
        <label class="msg-field">
          <span class="msg-field-label">Session DB path</span>
          <span class="msg-field-hint">Optional</span>
          <input class="msg-field-input msg-field-mono" bind:value={whatsappSessionDb} />
        </label>
      {/if}
    </div>
  </section>

  <section class="msg-story-chapter msg-story-chapter-quiet">
    <p class="msg-story-step">03</p>
    <div class="msg-story-chapter-body">
      <button
        type="button"
        class="msg-story-advanced"
        onclick={() => (advancedOpen = !advancedOpen)}
        aria-expanded={advancedOpen}
      >
        <span>
          <span class="msg-story-chapter-title">Advanced</span>
          <span class="msg-story-chapter-copy msg-story-chapter-copy-inline">
            Heartbeat nudges and delivery targets
          </span>
        </span>
        <span class="msg-story-chevron">{advancedOpen ? "▾" : "▸"}</span>
      </button>

      {#if advancedOpen}
        {#if channelId === "telegram"}
          <label class="msg-field-check msg-field-check-block">
            <input type="checkbox" class="checkbox" bind:checked={telegramHeartbeat} />
            Heartbeat nudges on configured chats
          </label>
          <label class="msg-field">
            <span class="msg-field-label">Heartbeat chat IDs</span>
            <input
              class="msg-field-input msg-field-mono"
              placeholder="-1001234567890"
              bind:value={telegramHeartbeatChats}
            />
          </label>
        {:else if channelId === "discord"}
          <label class="msg-field-check msg-field-check-block">
            <input type="checkbox" class="checkbox" bind:checked={discordHeartbeat} />
            Heartbeat nudges on configured channels
          </label>
          <label class="msg-field">
            <span class="msg-field-label">Heartbeat channel IDs</span>
            <input class="msg-field-input msg-field-mono" bind:value={discordHeartbeatChannels} />
          </label>
        {:else if channelId === "slack"}
          <label class="msg-field-check msg-field-check-block">
            <input type="checkbox" class="checkbox" bind:checked={slackHeartbeat} />
            Heartbeat nudges on configured channels
          </label>
          <label class="msg-field">
            <span class="msg-field-label">Heartbeat channel IDs</span>
            <input class="msg-field-input msg-field-mono" bind:value={slackHeartbeatChannels} />
          </label>
        {:else}
          <label class="msg-field-check msg-field-check-block">
            <input type="checkbox" class="checkbox" bind:checked={whatsappHeartbeat} />
            Heartbeat nudges on configured chats
          </label>
          <label class="msg-field">
            <span class="msg-field-label">Heartbeat chat JIDs</span>
            <input class="msg-field-input msg-field-mono" bind:value={whatsappHeartbeatJids} />
          </label>
        {/if}
      {/if}
    </div>
  </section>

  <footer class="msg-story-footer">
    <button
      type="button"
      class="btn btn-sm variant-filled-primary"
      disabled={saving}
      onclick={() => void onSave()}
    >
      {saving ? "Saving…" : "Save & connect"}
    </button>
    {#if saveMessage}
      <p class="msg-story-footer-note {saveMessage === 'Saved' ? 'is-ok' : 'is-warn'}">
        {saveMessage === "Saved"
          ? "Saved — Medousa starts this channel for you."
          : saveMessage}
      </p>
    {:else if !daemonOk}
      <p class="msg-story-footer-note">
        Engine offline — settings save; she connects when she’s back.
      </p>
    {/if}
  </footer>
</article>

<style>
  .msg-story {
    max-width: 32rem;
  }

  .msg-story-hero {
    display: flex;
    align-items: flex-start;
    gap: 0.9rem;
    padding-bottom: 0.25rem;
  }

  .msg-story-kicker {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-story-title {
    margin: 0.3rem 0 0;
    font-size: 1.35rem;
    font-weight: 600;
    letter-spacing: -0.03em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .msg-story-lead {
    margin: 0.45rem 0 0;
    max-width: 28rem;
    font-size: 0.875rem;
    line-height: 1.5;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .msg-story-chapter {
    display: grid;
    grid-template-columns: 1.75rem minmax(0, 1fr);
    gap: 0.85rem 1rem;
    margin-top: 2rem;
  }

  .msg-story-chapter-quiet {
    margin-top: 1.65rem;
  }

  .msg-story-step {
    margin: 0.2rem 0 0;
    font-size: 0.6875rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.08em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-story-chapter-title {
    margin: 0;
    display: block;
    font-size: 0.9375rem;
    font-weight: 600;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .msg-story-chapter-copy {
    margin: 0.4rem 0 0;
    font-size: 0.8125rem;
    line-height: 1.55;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .msg-story-chapter-copy-inline {
    display: block;
  }

  .msg-story-mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.78em;
    color: rgb(var(--shell-label, var(--color-surface-200)));
  }

  .msg-story-link {
    margin-left: 0.2rem;
    color: rgb(var(--color-primary-300));
    text-decoration: none;
    white-space: nowrap;
  }

  .msg-story-link:hover {
    text-decoration: underline;
  }

  .msg-field {
    display: block;
    margin-top: 1.15rem;
  }

  .msg-field-narrow {
    max-width: 6rem;
  }

  .msg-field-label {
    display: block;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--shell-label, var(--color-surface-200)));
  }

  .msg-field-hint {
    display: block;
    margin-top: 0.2rem;
    font-size: 0.71875rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-field-input {
    display: block;
    width: 100%;
    margin-top: 0.55rem;
    border: none;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    border-radius: 0;
    background: transparent;
    padding: 0.35rem 0 0.55rem;
    font-size: 0.875rem;
    color: rgb(var(--shell-label, var(--color-surface-50)));
    transition: border-color 140ms ease;
  }

  .msg-field-input::placeholder {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-field-input:focus {
    outline: none;
    border-bottom-color: rgb(var(--color-primary-500) / 0.7);
  }

  .msg-field-mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.8125rem;
    letter-spacing: 0.01em;
  }

  .msg-field-check {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.7rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
    cursor: pointer;
  }

  .msg-field-check-block {
    display: flex;
    margin-top: 1rem;
  }

  .msg-story-advanced {
    display: flex;
    width: 100%;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .msg-story-chevron {
    flex-shrink: 0;
    margin-top: 0.15rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .msg-story-footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem 1rem;
    margin-top: 2.25rem;
    padding-top: 1.15rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
  }

  .msg-story-footer-note {
    margin: 0;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .msg-story-footer-note.is-ok {
    color: rgb(var(--color-primary-300));
  }

  .msg-story-footer-note.is-warn {
    color: rgb(var(--color-warning-400));
  }
</style>
