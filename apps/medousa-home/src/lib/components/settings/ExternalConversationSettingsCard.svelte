<script lang="ts">
  import { onMount } from "svelte";
  import { Check, Plus, RefreshCw } from "@lucide/svelte";
  import {
    createExternalConversation,
    deleteExternalConversation,
    getMuseDiscovery,
    listExternalConversations,
    rotateExternalCallbackKey,
    startMuseDiscovery,
    type ExternalConversation,
    type ExternalProvider,
  } from "$lib/daemon/externalConversations";
  import type { ExternalMuseDiscoveryStatus } from "$lib/types/generated/daemon_api";

  let { provider }: { provider: ExternalProvider } = $props();
  let conversations = $state<ExternalConversation[]>([]);
  let creating = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let label = $state("");
  let target = $state("");
  let webhookUrl = $state("");
  let webhookKey = $state("");
  let callbackKey = $state<string | null>(null);
  let callbackConversationId = $state<string | null>(null);
  let confirmDeleteId = $state<string | null>(null);
  let discovery = $state<ExternalMuseDiscoveryStatus | null>(null);
  const title = $derived(provider === "muse" ? "Muse" : "Grok Bot");
  const registered = $derived(conversations.filter((item) => item.provider === provider));

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

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible" || busy || !creating || provider !== "muse" || !discovery) return;
      void getMuseDiscovery().then((status) => {
        discovery = status;
      }).catch(() => {
        discovery = null;
        error = "Muse chat discovery expired. Start it again.";
      });
    }, 3000);
    return () => window.clearInterval(timer);
  });

  async function discoverMuse() {
    if (busy) return;
    busy = true;
    error = null;
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
      });
      conversations = [result.conversation, ...conversations];
      callbackKey = result.callback_key ?? null;
      callbackConversationId = result.conversation.id;
      webhookKey = "";
      label = "";
      target = "";
      webhookUrl = "";
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
    <span class="connections-card-icon" aria-hidden="true">{provider === "muse" ? "M" : "G"}</span>
    <div class="min-w-0 flex-1">
      <p class="connections-card-title">{title}</p>
      <p class="connections-card-sub workshop-faint">{provider === "muse" ? "WhatsApp sessions" : "Webhook bots and routines"}</p>
    </div>
    <span class="connections-status" class:connections-status--in={registered.length > 0}>
      {#if registered.length > 0}<Check size={12} strokeWidth={2.5} />{/if}
      {registered.length > 0 ? registered.length + " connected" : "Not connected"}
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
    <div class="mt-3 space-y-2">
      {#each registered as conversation (conversation.id)}
        <div class="rounded-lg bg-surface-800 p-3">
          <p class="text-sm font-medium text-surface-50">{conversation.label}</p>
          <p class="workshop-faint mt-1 text-xs">{provider === "muse" ? "Session" : "Bot · " + conversation.target}</p>
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
      <label class="block text-sm">{provider === "muse" ? "Session name" : "Bot label"}
        <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={label} required maxlength="120" />
      </label>
      {#if provider === "grok_bot"}
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
        <p class="workshop-faint text-xs">Pair WhatsApp in Settings → Packages on the connected workshop, then ask Muse in WhatsApp to reply with the code below.</p>
        <button type="button" class="text-content-link text-sm" disabled={busy} onclick={() => void discoverMuse()}>{discovery ? "Restart discovery" : "Find Muse chat"}</button>
        {#if discovery}
          <code class="block break-all select-all text-xs">{discovery.challenge}</code>
          <p class="workshop-faint text-xs">Send: “Reply with exactly {discovery.challenge}”</p>
          <p class="text-xs">{discovery.observed_chat_jid ? "Chat observed. You can connect this session." : "Waiting for Muse’s reply. Discovery expires after five minutes."}</p>
        {/if}
      {/if}
      <div class="flex gap-3">
        <button type="submit" class="btn btn-sm variant-filled-primary" disabled={busy || (provider === "muse" && !discovery?.observed_chat_jid)}>Connect</button>
        <button type="button" class="btn btn-sm variant-soft-surface" onclick={() => creating = false}>Cancel</button>
      </div>
    </form>
  {:else}
    <div class="connections-card-actions">
      <button type="button" class="btn btn-sm variant-filled-primary" onclick={() => creating = true}><Plus size={13} strokeWidth={2} /> {provider === "muse" ? "Add session" : "Add bot"}</button>
      <button type="button" class="btn btn-sm variant-soft-surface" disabled={busy} onclick={() => void refresh()}><RefreshCw size={13} strokeWidth={2} /> Refresh</button>
    </div>
  {/if}
</div>

<style>
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
