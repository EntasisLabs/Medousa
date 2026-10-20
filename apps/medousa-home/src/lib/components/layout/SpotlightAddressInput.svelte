<script lang="ts">
  import { X } from "@lucide/svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
  import { conversationTargets, mentionQuery, type ConversationTarget } from "$lib/spotlight/conversationTargets";
  import SpotlightTargetPicker from "./SpotlightTargetPicker.svelte";
  let { query = $bindable(""), inputEl = $bindable<HTMLInputElement | null>(null), conversation, placeholder, busy, enabled, onQueryChange }: {
    query: string; inputEl: HTMLInputElement | null; conversation: AddressedConversation; placeholder: string; busy: boolean; enabled: boolean; onQueryChange: () => void;
  } = $props();
  let index = $state(0);
  let dismissed = $state(false);
  const mention = $derived(enabled && !conversation.target ? mentionQuery(query) : null);
  const picking = $derived(Boolean(mention) && !dismissed);
  const targets = $derived(conversationTargets(chat.workshopScopeId ?? "", bots.bots, chat.sessions, conversation.runtimes, mention?.name ?? ""));
  function focus() { requestAnimationFrame(() => inputEl?.focus()); }
  function select(target: ConversationTarget) {
    const text = mention?.message ?? "";
    conversation.select(target); query = text; index = 0; dismissed = false; focus();
  }
  function remove() { conversation.clear(); query = ""; dismissed = false; focus(); }
  async function send() {
    if (!query.trim() || conversation.sending) return;
    const text = query; const target = conversation.target;
    try { await conversation.send(text, () => { if (conversation.target === target && query === text) query = ""; }); }
    catch { /* Inline error preserves the draft. */ }
    focus();
  }
  export function handleKeydown(event: KeyboardEvent): boolean {
    if (event.isComposing || !enabled || (event.key === "Tab" && event.shiftKey)) return false;
    if (picking && event.key === "Escape") { event.preventDefault(); dismissed = true; return true; }
    if (event.target !== inputEl || (!mention && !conversation.target)) return false;
    if (picking && ["ArrowDown", "ArrowUp", "Enter", "Tab"].includes(event.key)) {
      event.preventDefault();
      if (event.key === "Enter" || event.key === "Tab") { if (targets[index]) select(targets[index]); }
      else if (targets.length) index = (index + (event.key === "ArrowDown" ? 1 : -1) + targets.length) % targets.length;
      return true;
    }
    if (conversation.target && event.key === "Backspace" && !query) { event.preventDefault(); remove(); return true; }
    if (event.key === "Enter") { event.preventDefault(); if (conversation.target) void send(); return true; }
    return event.key !== "Escape";
  }
</script>

<div class="command-spotlight-search-row">
  {#if conversation.target}<button type="button" class="command-spotlight-recipient" aria-label={`Remove destination ${conversation.target.label}`} onclick={remove}><span>@{conversation.target.label}</span><X size={12}/></button>{/if}
  <input bind:this={inputEl} class="command-spotlight-input"
    placeholder={conversation.target ? `Message ${conversation.target.label}…` : placeholder}
    aria-label={conversation.target ? `Message ${conversation.target.label}` : "Search or address a conversation"}
    role="combobox" aria-expanded={picking} aria-autocomplete="list"
    aria-controls={picking ? "spotlight-targets" : undefined}
    aria-activedescendant={picking && targets[index] ? `spotlight-target-${index}` : undefined}
    bind:value={query} oninput={() => { index = 0; dismissed = false; onQueryChange(); }} disabled={busy}/>
  {#if picking}<SpotlightTargetPicker {targets} {index} loading={bots.loading} error={bots.error} onSelect={select} onHighlight={value => index = value}/>{/if}
</div>
