<script lang="ts">
  import { onMount } from "svelte";
  import ExternalAgentAccessControls from "./ExternalAgentAccessControls.svelte";
  import { Check, Plus, RefreshCw } from "@lucide/svelte";
  import { getDaemonUrl } from "$lib/daemon/client";
  import { ensureWhatsAppAdapter } from "$lib/messaging";
  import { isTauriDesktop } from "$lib/platform";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import {
    createExternalConversation,
    deleteExternalConversation,
    getMuseDiscovery,
    getWhatsAppPairingStatus,
    listExternalConversations,
    rotateExternalCallbackKey,
    startMuseDiscovery,
    type ExternalConversation,
    type ExternalProvider,
  } from "$lib/daemon/externalConversations";
  import type { ExternalMuseDiscoveryStatus, ExternalWhatsAppPairingStatus } from "$lib/types/generated/daemon_api";

  let { provider }: { provider: ExternalProvider } = $props();
  let conversations = $state<ExternalConversation[]>([]);
  let creating = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let label = $state("");
  let target = $state("");
  let webhookUrl = $state("");
  let webhookKey = $state("");
  let slackUserToken = $state("");
  let dotUserId = $state("");
  let callbackKey = $state<string | null>(null);
  let callbackConversationId = $state<string | null>(null);
  let confirmDeleteId = $state<string | null>(null);
  let discovery = $state<ExternalMuseDiscoveryStatus | null>(null);
  let discoveryFeedback = $state<string | null>(null);
  let pairing = $state<ExternalWhatsAppPairingStatus | null>(null);
  let pairingError = $state<string | null>(null);
  let adapterError = $state<string | null>(null);
  let adapterBusy = $state(false);
  let pairingBusy = false;
  const title = $derived(provider === "muse" ? "Muse" : provider === "instinct" ? "Instinct Agent" : provider === "dots" ? "Dots" : "Grok Bot");
  const registered = $derived(conversations.filter((item) => item.provider === provider));
  const canStartAdapter = $derived(isTauriDesktop() && isCoLocatedWorkshop());

  function changed() {
    window.dispatchEvent(new Event("medousa-external-conversation-changed"));
  }

  async function refresh() {
    try {
      conversations = await listExternalConversations();
      if (!creating) error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function refreshPairing() {
    if (pairingBusy) return;
    pairingBusy = true;
    try {
      pairing = await getWhatsAppPairingStatus();
      pairingError = null;
    } catch (cause) {
      pairing = null;
      pairingError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      pairingBusy = false;
    }
  }

  async function startWhatsAppAdapter() {
    if (!canStartAdapter || adapterBusy) return;
    adapterBusy = true;
    adapterError = null;
    try {
      await ensureWhatsAppAdapter(await getDaemonUrl());
      await refreshPairing();
    } catch (cause) {
      adapterError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      adapterBusy = false;
    }
  }

  async function openWhatsAppSetup() {
    creating = true;
    await refreshPairing();
    if (!pairing || pairing.state === "waiting") {
      await startWhatsAppAdapter();
    }
  }

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible" || !creating || provider === "grok_bot" || provider === "dots") return;
      void refreshPairing();
      if (busy || provider !== "muse" || !discovery) return;
      void checkMuseDiscovery();
    }, 3000);
    return () => window.clearInterval(timer);
  });

  async function checkMuseDiscovery(manual = false) {
    const challenge = discovery?.challenge;
    if (!challenge) return;
    if (manual) discoveryFeedback = "Checking whether the linked adapter saw your code…";
    try {
      const status = await getMuseDiscovery();
      if (discovery?.challenge !== challenge) return;
      discovery = status;
      if (status.observed_chat_jid) {
        error = null;
        discoveryFeedback = null;
      } else if (manual) {
        discoveryFeedback = "The linked adapter has not seen this code yet.";
      }
    } catch (cause) {
      if (discovery?.challenge !== challenge) return;
      discovery = null;
      discoveryFeedback = null;
      error = cause instanceof Error ? cause.message : "Muse chat discovery expired. Start it again.";
    }
  }

  async function discoverMuse() {
    if (busy) return;
    busy = true;
    error = null;
    discoveryFeedback = null;
    try {
      discovery = await startMuseDiscovery();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function create() {
    if (busy || !label.trim() || (provider === "muse" && !discovery?.observed_chat_jid)) return;
    busy = true;
    error = null;
    try {
      const result = await createExternalConversation({
        provider,
        label: label.trim(),
        target: provider === "muse" ? discovery!.observed_chat_jid! : target.trim(),
        ...(provider === "grok_bot"
          ? { webhook_url: webhookUrl.trim(), webhook_key: webhookKey.trim() }
          : {}),
        ...(provider === "dots" ? { dot_user_id: dotUserId.trim(), slack_user_token: slackUserToken.trim() } : {}),
      });
      conversations = [result.conversation, ...conversations];
      callbackKey = result.callback_key ?? null;
      callbackConversationId = result.conversation.id;
      webhookKey = "";
      label = "";
      target = "";
      webhookUrl = "";
      slackUserToken = "";
      dotUserId = "";
      discovery = null;
      creating = false;
      changed();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function rotate(id: string) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      callbackKey = (await rotateExternalCallbackKey(id)).callback_key;
      callbackConversationId = id;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function remove(id: string) {
    if (busy) return;
    if (confirmDeleteId !== id) {
      confirmDeleteId = id;
      return;
    }
    busy = true;
    error = null;
    try {
      await deleteExternalConversation(id);
      conversations = conversations.filter((item) => item.id !== id);
      if (callbackConversationId === id) {
        callbackKey = null;
        callbackConversationId = null;
      }
      confirmDeleteId = null;
      changed();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<div class="connections-card" data-provider={provider}>
  <div class="connections-card-head">
    <span class="connections-card-icon" aria-hidden="true">{title[0]}</span>
    <div class="min-w-0 flex-1">
      <p class="connections-card-title">{title}</p>
      <p class="connections-card-sub workshop-faint">{provider === "grok_bot" ? "Webhook bots and routines" : provider === "dots" ? "Slack conversations" : "WhatsApp sessions"}</p>
    </div>
    <span class="connections-status" class:connections-status--in={provider === "grok_bot" && registered.length > 0}>
      {#if provider === "grok_bot" && registered.length > 0}<Check size={12} strokeWidth={2.5} />{/if}
      {provider !== "grok_bot" ? registered.length + " registered" : registered.length > 0 ? registered.length + " connected" : "Not connected"}
    </span>
  </div>

  {#if error}<p role="alert" class="mt-2 text-sm text-content-error">{error}</p>{/if}

  {#if callbackKey && callbackConversationId}
    <div class="mt-3 rounded-lg border border-content-warning p-3 text-sm">
      <p class="font-medium">Save this callback key now. It is shown once.</p>
      <p class="workshop-faint mt-1">The agent VM must send authenticated events to <code>/v1/external-conversations/{callbackConversationId}/events</code> with <code>x-medousa-bridge-key</code> and a paired workshop credential.</p>
      <code class="mt-2 block break-all select-all">{callbackKey}</code>
      <button type="button" class="mt-2 text-content-link" onclick={() => callbackKey = null}>Done</button>
    </div>
  {/if}

  {#if registered.length > 0}
    {#if provider === "muse"}
      <p class="text-xs text-content-warning">WhatsApp may mark Medousa messages Read without delivering them to Muse. In our linked-device test, the message did not appear in Muse's app and replies did not reach Medousa.</p>
    {/if}
    <div class="mt-3 space-y-2">
      {#each registered as conversation (conversation.id)}
        <div class="rounded-lg bg-surface-800 p-3">
          <p class="text-sm font-medium text-surface-50">{conversation.label}</p>
          <p class="workshop-faint mt-1 text-xs">{provider === "muse" ? "Session" : provider === "instinct" ? "WhatsApp · +" + conversation.target.split("@")[0] : provider === "dots" ? "Slack · " + conversation.target : "Bot · " + conversation.target}</p>
          {#if provider === "instinct" || provider === "dots"}
            <ExternalAgentAccessControls {conversation} onchange={(updated) => { conversations = conversations.map((item) => item.id === updated.id ? updated : item); }} />
          {/if}
          <div class="mt-2 flex flex-wrap gap-3 text-xs">
            {#if provider === "grok_bot"}
              <button type="button" class="text-content-link" disabled={busy} onclick={() => void rotate(conversation.id)}>Rotate callback key</button>
            {/if}
            <button type="button" class="text-content-warning" disabled={busy} onclick={() => void remove(conversation.id)}>{confirmDeleteId === conversation.id ? "Confirm removal" : "Remove"}</button>
            {#if confirmDeleteId === conversation.id}<button type="button" class="workshop-faint" onclick={() => confirmDeleteId = null}>Cancel</button>{/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if creating}
    <form class="mt-3 space-y-3" onsubmit={(event) => { event.preventDefault(); void create(); }}>
      <label class="block text-sm">{provider === "grok_bot" ? "Bot label" : "Session name"}
        <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={label} required maxlength="120" />
      </label>
      {#if provider === "dots"}
        <p class="workshop-faint text-xs">Add your dot and the Medousa Slack app to a dedicated channel. The outgoing user token must belong to the dot’s owner and have chat:write access. Install the Slack adapter in Settings → Packages and configure it in Settings → Sharing → Channels.</p>
        <label class="block text-sm">Slack channel ID
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={target} placeholder="C0123456789" required maxlength="32" />
        </label>
        <label class="block text-sm">Dot Slack member or bot ID
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={dotUserId} placeholder="U0123456789" required maxlength="32" />
        </label>
        <label class="block text-sm">Your Slack user token
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="password" bind:value={slackUserToken} required autocomplete="off" />
        </label>
      {:else if provider === "grok_bot"}
        <label class="block text-sm">Bot name
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={target} required maxlength="256" />
        </label>
        <label class="block text-sm">Webhook POST URL
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="url" bind:value={webhookUrl} required />
        </label>
        <label class="block text-sm">Webhook key
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="password" bind:value={webhookKey} required autocomplete="off" />
        </label>
      {:else}
        <p class="workshop-faint text-xs">Install the WhatsApp adapter in Settings → Packages. Configure WhatsApp in Settings → Sharing → Channels with deliver bind <code>127.0.0.1:7423</code>. {canStartAdapter ? "Medousa starts the adapter when you add a session." : "Start the adapter on the workshop machine."}</p>
        {#if pairing?.state === "qr_ready" && pairing.qr_svg && pairing.expires_at && Date.parse(pairing.expires_at) > Date.now()}
          <div class="muse-pairing-qr">
            <img src={`data:image/svg+xml,${encodeURIComponent(pairing.qr_svg)}`} alt="WhatsApp Linked Devices pairing QR" />
            <p class="text-xs">On your phone: WhatsApp → Settings → Linked Devices → Link a Device. Scan this QR from Medousa. It refreshes automatically when it expires.</p>
          </div>
        {:else if pairing?.state === "connected"}
          <p class="text-xs text-content-success">WhatsApp linked. {provider === "muse" ? "You can find the Muse chat below." : "Enter Instinct’s international phone number below."}</p>
        {:else if pairing?.state === "logged_out"}
          <p class="text-xs text-content-warning">WhatsApp signed out. Save WhatsApp again in Settings → Sharing → Channels to restart the adapter and get a new QR.</p>
        {:else}
          <p class="workshop-faint text-xs">Waiting for the workshop’s WhatsApp pairing QR. If WhatsApp is already connected and this stays blank, update the WhatsApp adapter in Settings → Packages and restart it.</p>
        {/if}
        {#if pairingError}<p class="workshop-faint text-xs">Pairing status unavailable: {pairingError}</p>{/if}
        {#if adapterError}<p class="text-xs text-content-warning">WhatsApp adapter could not start: {adapterError}</p>{/if}
        {#if canStartAdapter && (!pairing || pairing.state === "waiting")}
          <button type="button" class="text-content-link text-sm" disabled={adapterBusy} onclick={() => void startWhatsAppAdapter()}>{adapterBusy ? "Starting WhatsApp adapter…" : "Start WhatsApp adapter"}</button>
        {/if}
        {#if provider === "instinct"}
          <label class="block text-sm">Instinct WhatsApp phone number
            <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="tel" bind:value={target} placeholder="+1 555 123 4567" required maxlength="40" />
          </label>
          <p class="workshop-faint text-xs">Include the country code. After connecting, create an API token if Instinct needs workshop tools or background work.</p>
        {:else}
        <div class="flex flex-wrap gap-3">
          <button type="button" class="text-content-link text-sm" disabled={busy} onclick={() => void discoverMuse()}>{discovery ? "Restart discovery" : "Find Muse chat"}</button>
          {#if discovery && !discovery.observed_chat_jid}
            <button type="button" class="text-content-link text-sm" disabled={busy} onclick={() => void checkMuseDiscovery(true)}>Check for code</button>
          {/if}
        </div>
        {#if discovery}
          <code class="block break-all select-all text-xs">{discovery.challenge}</code>
          <p class="workshop-faint text-xs">Send this code in your Muse chat on WhatsApp: {discovery.challenge}</p>
          <p class="text-xs">{discovery.observed_chat_jid ? "Chat ID observed. This does not verify delivery to Muse." : "Waiting for the linked adapter to see your code. Discovery expires after five minutes."}</p>
          {#if discoveryFeedback}<p class="workshop-faint text-xs">{discoveryFeedback}</p>{/if}
        {/if}
        {/if}
      {/if}
      <div class="flex gap-3">
        <button type="submit" class="btn btn-sm variant-filled-primary" disabled={busy || (provider === "muse" && !discovery?.observed_chat_jid)}>Connect</button>
        <button type="button" class="btn btn-sm variant-soft-surface" onclick={() => creating = false}>Cancel</button>
      </div>
    </form>
  {:else}
    <div class="connections-card-actions">
      <button type="button" class="btn btn-sm variant-filled-primary" onclick={() => { if (provider === "muse" || provider === "instinct") void openWhatsAppSetup(); else creating = true; }}><Plus size={13} strokeWidth={2} /> {provider === "grok_bot" ? "Add bot" : "Add session"}</button>
      <button type="button" class="btn btn-sm variant-soft-surface" disabled={busy} onclick={() => void refresh()}><RefreshCw size={13} strokeWidth={2} /> Refresh</button>
    </div>
  {/if}
</div>

<style>
  .muse-pairing-qr {
    display: grid;
    justify-items: center;
    gap: 0.65rem;
    border-radius: 0.75rem;
    background: white;
    color: #171717;
    padding: 1rem;
    text-align: center;
  }
  .muse-pairing-qr img {
    width: min(100%, 280px);
    height: auto;
    image-rendering: pixelated;
  }
  .connections-card {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    background: rgb(var(--color-surface-900) / 0.5);
    padding: 0.8rem 0.9rem;
  }
  .connections-card-head, .connections-card-actions {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }
  .connections-card-actions { flex-wrap: wrap; }
  .connections-card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-500) / 0.18);
    color: rgb(var(--color-surface-200));
    flex-shrink: 0;
    font-weight: 600;
  }
  .connections-card-title { margin: 0; font-size: 0.875rem; font-weight: 600; color: rgb(var(--color-surface-100)); }
  .connections-card-sub { margin: 0.1rem 0 0; font-size: 0.72rem; }
  .connections-status {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
    font-size: 0.68rem;
    font-weight: 600;
    background: rgb(var(--color-surface-500) / 0.22);
    color: rgb(var(--theme-text-secondary));
  }
  .connections-status--in { background: rgb(var(--color-success-500) / 0.18); color: rgb(var(--theme-success)); }
</style>
