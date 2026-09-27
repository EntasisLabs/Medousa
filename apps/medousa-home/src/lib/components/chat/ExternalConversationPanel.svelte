<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeft, Plus, RefreshCw, Send } from "@lucide/svelte";
  import {
    createExternalConversation,
    getExternalConversation,
    listExternalConversations,
    rotateExternalCallbackKey,
    deleteExternalConversation,
    sendExternalConversationMessage,
    type ExternalConversation,
    type ExternalProvider,
  } from "$lib/daemon/externalConversations";

  let { onClose }: { onClose: () => void } = $props();
  let conversations = $state<ExternalConversation[]>([]);
  let selectedId = $state<string | null>(null);
  let creating = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let draft = $state("");
  let provider = $state<ExternalProvider>("grok_bot");
  let label = $state("");
  let target = $state("");
  let webhookUrl = $state("");
  let webhookKey = $state("");
  let callbackKey = $state<string | null>(null);
  let confirmDelete = $state(false);
  const selected = $derived(conversations.find((item) => item.id === selectedId) ?? null);

  function statusFor(conversation: ExternalConversation): string {
    const last = conversation.events.at(-1);
    if (!last) return "Needs live verification";
    if (last.kind === "transport_pending") return "Sending · outcome unknown after restart";
    if (last.kind === "transport_uncertain") return "Check provider history before resending";
    if (last.kind === "transport_failed") return "Send failed";
    if (last.kind === "transport_accepted") return "Accepted · awaiting provider reply";
    if (last.kind === "completed") return "Completed · reported by provider";
    if (last.kind === "failed") return "Failed · reported by provider";
    return "Provider activity received";
  }

  function upsert(conversation: ExternalConversation) {
    conversations = [conversation, ...conversations.filter((item) => item.id !== conversation.id)];
  }

  async function refresh() {
    try {
      if (selectedId) {
        upsert(await getExternalConversation(selectedId));
      } else {
        conversations = await listExternalConversations();
      }
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible" && !busy) void refresh();
    }, 3000);
    return () => window.clearInterval(timer);
  });

  async function create() {
    if (busy || !label.trim()) return;
    busy = true;
    error = null;
    try {
      const museTarget = target.trim().includes("@")
        ? target.trim()
        : `${target.replace(/\D/g, "")}@s.whatsapp.net`;
      const result = await createExternalConversation({
        provider,
        label: label.trim(),
        target: provider === "muse" ? museTarget : target.trim(),
        ...(provider === "grok_bot"
          ? { webhook_url: webhookUrl.trim(), webhook_key: webhookKey.trim() }
          : {}),
      });
      webhookKey = "";
      callbackKey = result.callback_key ?? null;
      upsert(result.conversation);
      selectedId = result.conversation.id;
      creating = false;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function send() {
    const text = draft.trim();
    if (!selectedId || !text || busy) return;
    const id = selectedId;
    busy = true;
    error = null;
    try {
      upsert(await sendExternalConversationMessage(id, text, crypto.randomUUID()));
      draft = "";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      void refresh();
    } finally {
      busy = false;
    }
  }

  async function rotateCallback() {
    if (!selectedId || busy) return;
    busy = true;
    error = null;
    try {
      callbackKey = (await rotateExternalCallbackKey(selectedId)).callback_key;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function removeConversation() {
    if (!selectedId || busy) return;
    if (!confirmDelete) { confirmDelete = true; return; }
    busy = true;
    error = null;
    try {
      await deleteExternalConversation(selectedId);
      conversations = conversations.filter((item) => item.id !== selectedId);
      selectedId = null;
      callbackKey = null;
      confirmDelete = false;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<div class="absolute inset-0 z-30 flex min-h-0 flex-col bg-surface-950 text-surface-50">
  <header class="flex items-center gap-2 border-b border-surface-800 px-4 py-3">
    <button type="button" class="workshop-rail-btn" aria-label="Back to chat" onclick={onClose}>
      <ArrowLeft size={16} />
    </button>
    <div class="min-w-0 flex-1">
      <h2 class="truncate text-sm font-semibold">{selected?.label ?? "Provider conversations"}</h2>
      <p class="workshop-faint text-xs">Muse via WhatsApp · Grok Bot via webhook</p>
    </div>
    <button type="button" class="workshop-rail-btn" aria-label="Refresh conversations" onclick={() => void refresh()}>
      <RefreshCw size={15} />
    </button>
  </header>

  {#if error}<p role="alert" class="px-4 py-2 text-sm text-content-error">{error}</p>{/if}

  {#if callbackKey}
    <div class="m-4 rounded-lg border border-content-warning p-3 text-sm">
      <p class="font-medium">Save this callback key now. It is shown once.</p>
      <p class="workshop-faint mt-1">Pair the agent VM with this workshop, then send authenticated events to
        <code>/v1/external-conversations/{selectedId}/events</code> with
        <code>x-medousa-bridge-key</code>. The callback also needs a paired workshop credential.</p>
      <code class="mt-2 block break-all select-all">{callbackKey}</code>
      <button type="button" class="mt-2 text-content-link" onclick={() => callbackKey = null}>Done</button>
    </div>
  {/if}

  {#if creating}
    <form class="mx-auto flex w-full max-w-xl flex-col gap-3 overflow-auto p-5" onsubmit={(event) => { event.preventDefault(); void create(); }}>
      <h3 class="font-semibold">Connect an agent</h3>
      <label class="text-sm">Agent
        <select class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={provider}>
          <option value="grok_bot">Grok Bot</option>
          <option value="muse">Muse</option>
        </select>
      </label>
      <label class="text-sm">Conversation name
        <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={label} required maxlength="120" />
      </label>
      <label class="text-sm">{provider === "muse" ? "Muse WhatsApp number or chat JID" : "Bot name"}
        <input class="mt-1 w-full rounded-md bg-surface-800 p-2" bind:value={target} required maxlength="256" />
      </label>
      {#if provider === "grok_bot"}
        <label class="text-sm">Webhook POST URL
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="url" bind:value={webhookUrl} required />
        </label>
        <label class="text-sm">Webhook key
          <input class="mt-1 w-full rounded-md bg-surface-800 p-2" type="password" bind:value={webhookKey} required autocomplete="off" />
        </label>
      {:else}
        <p class="workshop-faint text-xs">Pair the WhatsApp package on the connected workshop first. Use the exact Muse chat number.</p>
      {/if}
      <div class="flex gap-3">
        <button type="submit" class="rounded-md bg-primary-600 px-4 py-2 text-sm" disabled={busy}>Connect</button>
        <button type="button" class="text-sm" onclick={() => creating = false}>Cancel</button>
      </div>
    </form>
  {:else if selected}
    <div class="flex-1 overflow-auto px-4 py-5">
      <div class="mb-4 flex flex-wrap items-center gap-4 text-xs">
        <button type="button" class="text-content-link" onclick={() => { selectedId = null; callbackKey = null; confirmDelete = false; void refresh(); }}>All conversations</button>
        {#if selected.provider === "grok_bot"}
          <button type="button" class="text-content-link" disabled={busy} onclick={() => void rotateCallback()}>Rotate callback key</button>
        {/if}
        <button type="button" class="text-content-warning" disabled={busy} onclick={() => void removeConversation()}>{confirmDelete ? "Confirm removal" : "Remove conversation"}</button>
        {#if confirmDelete}<button type="button" class="workshop-faint" onclick={() => confirmDelete = false}>Cancel</button>{/if}
      </div>
      <div class="mx-auto flex max-w-2xl flex-col gap-3">
        {#each selected.events as event (event.sequence)}
          <div class="max-w-[88%] rounded-lg px-3 py-2 text-sm {event.kind === 'user_message' ? 'self-end bg-primary-700' : event.kind.startsWith('transport_') ? 'self-end bg-surface-800 text-content-tertiary' : 'self-start bg-surface-800'}">
            <p class="mb-1 text-[11px] opacity-70">{event.kind.replaceAll("_", " ")}</p>
            <p class="whitespace-pre-wrap break-words">{event.text}</p>
          </div>
        {/each}
      </div>
    </div>
    <form class="flex gap-2 border-t border-surface-800 p-3" onsubmit={(event) => { event.preventDefault(); void send(); }}>
      <input class="min-w-0 flex-1 rounded-md bg-surface-800 p-2 text-sm" bind:value={draft} placeholder="Message {selected.label}" aria-label="Message {selected.label}" />
      <button type="submit" class="rounded-md bg-primary-600 px-3" aria-label="Send message" disabled={busy || !draft.trim()}><Send size={16} /></button>
    </form>
  {:else}
    <div class="flex-1 overflow-auto p-4">
      <div class="mx-auto max-w-2xl space-y-2">
        {#each conversations as conversation (conversation.id)}
          <button type="button" class="flex w-full items-center justify-between rounded-lg bg-surface-800 p-3 text-left" onclick={() => selectedId = conversation.id}>
            <span><strong class="block text-sm">{conversation.label}</strong><small class="workshop-faint">{conversation.provider === "muse" ? "Muse" : "Grok Bot"} · {statusFor(conversation)}</small></span>
            <span class="workshop-faint text-xs">Open</span>
          </button>
        {/each}
        <button type="button" class="flex items-center gap-2 rounded-md bg-primary-600 px-4 py-2 text-sm" onclick={() => creating = true}><Plus size={15} /> Connect agent</button>
      </div>
    </div>
  {/if}
</div>
