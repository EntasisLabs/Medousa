<script lang="ts">
  import "$lib/styles/command-spotlight.postcss";
  import { untrack, type Snippet } from "svelte";
  import { ArrowUpRight, Bot, ClipboardPaste, Globe, House, LayoutGrid, MessageSquare, Minus, MoreHorizontal, Plus, Send, Sparkles, StickyNote, X } from "@lucide/svelte";
  import SpotlightAddressInput from "$lib/components/layout/SpotlightAddressInput.svelte";
  import SpotlightTranscript from "$lib/components/layout/SpotlightTranscript.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { companionResults, type CompanionAction, type CompanionResult } from "$lib/companion/companionResults";
  import type { CompanionConversation } from "$lib/companion/companionConversation.svelte";
  let { model, connected, onOpen, onClipboard, onCollapse, onHide = () => {}, onMain = () => {}, onBodyChange = () => {}, actions = [], scopeLabel = "Home", notice, hasNotice = false }: {
    model: CompanionConversation; connected: boolean; onOpen: () => void; onClipboard: () => void; onCollapse: () => void;
    onHide?: () => void; onMain?: () => void; onBodyChange?: (value: boolean) => void;
    actions?: CompanionAction[]; scopeLabel?: string; notice?: Snippet; hasNotice?: boolean;
  } = $props();
  let field: SpotlightAddressInput;
  let inputEl = $state<HTMLInputElement | null>(null);
  let resultList = $state<HTMLDivElement | null>(null);
  let requestedBody = $state(false);
  let picking = $state(false);
  let menuOpen = $state(false);
  let index = $state(0);
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  const conversation = $derived(model.conversation);
  const enabled = $derived(connected && model.available);
  const results = $derived(companionResults(model.query, chat.workshopScopeId ?? "", bots.bots, chat.sessions, conversation.runtimes, actions));
  const bodyOpen = $derived(Boolean(menuOpen || requestedBody || model.query || conversation.target || model.restoreError || actionError || hasNotice));
  const searching = $derived(bodyOpen && !picking && !conversation.target);
  const actionIcons = { chat: MessageSquare, note: StickyNote, web: Globe, view: LayoutGrid, main: House };
  $effect(() => { onBodyChange(bodyOpen); });
  $effect(() => { if (enabled && inputEl) inputEl.focus(); });
  $effect(() => {
    const active = index;
    if (resultList) resultList.querySelector<HTMLElement>(`[data-result-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  });
  $effect(() => { if (index >= results.length) index = 0; });
  async function choose(result: CompanionResult) {
    if (actionBusy || !enabled) return;
    actionError = null; menuOpen = false;
    if (result.kind === "ask") { await field.send(); return; }
    if (result.kind === "target") {
      conversation.select(result.target); model.query = ""; requestedBody = true; inputEl?.focus(); return;
    }
    const query = model.query;
    actionBusy = true;
    try { await result.action.run(); if (model.query === query) model.query = ""; }
    catch (cause) { actionError = cause instanceof Error ? cause.message : String(cause); }
    finally { actionBusy = false; }
  }
  function keydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === "Escape" && menuOpen) { event.preventDefault(); menuOpen = false; return; }
    if (event.target === inputEl && enabled && !conversation.target && !model.query.startsWith("@") && ["ArrowDown", "ArrowUp", "Enter"].includes(event.key)) {
      if (!bodyOpen && event.key !== "Enter") { event.preventDefault(); requestedBody = true; index = 0; return; }
      if (searching && results.length) {
        event.preventDefault();
        if (event.key === "Enter") void choose(results[index] ?? results[0]);
        else index = (index + (event.key === "ArrowDown" ? 1 : -1) + results.length) % results.length;
        return;
      }
    }
    if (field?.handleKeydown(event)) { event.stopPropagation(); return; }
    if (event.key === "Escape") { event.preventDefault(); onCollapse(); }
  }
  function queryChanged() { requestedBody = true; index = 0; actionError = null; }
  function startNew() { model.query = ""; model.startNew(); menuOpen = false; inputEl?.focus(); }
  function outside(event: PointerEvent) {
    if (!menuOpen || !(event.target instanceof Element)) return;
    if (!event.target.closest('.bar-menu') && !event.target.closest('[aria-label="More companion actions"]')) menuOpen = false;
  }
  // Keep the user's target and draft in the route-owned model when this UI unmounts.
  $effect(() => () => { untrack(() => onBodyChange(false)); });
</script>

<svelte:window onkeydown={keydown} onpointerdown={outside}/>
<section class="companion-conversation" class:body-open={bodyOpen} aria-label="Mini Spotlight">
  {#snippet trailing()}
    {#if conversation.target}<button type="button" class="bar-icon" aria-label="Send message" onclick={() => void field?.send()} disabled={!enabled || conversation.sending || !model.query.trim()}><Send size={14}/></button>{/if}
    <button type="button" class="bar-icon" aria-label="More companion actions" aria-expanded={menuOpen} onclick={() => menuOpen = !menuOpen}><MoreHorizontal size={16}/></button>
    <button type="button" class="bar-icon" aria-label="Collapse companion" onclick={onCollapse}><Minus size={16}/></button>
  {/snippet}
  <SpotlightAddressInput bind:this={field} bind:query={model.query} bind:inputEl {conversation}
    placeholder={connected ? "Ask, search, or @someone…" : "Waiting for the workshop…"}
    busy={!enabled || conversation.sending || actionBusy} {enabled} idPrefix="companion" {trailing}
    inlinePicker onPickingChange={value => picking = value}
    resultsOpen={searching} activeResultId={searching && results[index] ? `companion-result-${index}` : undefined}
    onActivate={() => requestedBody = true} defaultTarget={model.restoreError ? undefined : () => model.newTarget()} onQueryChange={queryChanged}/>
  {#if menuOpen}
    <div class="bar-menu" aria-label="Companion actions">
      <button type="button" onclick={startNew} disabled={!enabled || conversation.sending}><Plus size={14}/>New request</button>
      <button type="button" onclick={() => { menuOpen = false; onClipboard(); inputEl?.focus(); }} disabled={!enabled}><ClipboardPaste size={14}/>Use clipboard</button>
      <button type="button" onclick={() => { menuOpen = false; onMain(); }}><ArrowUpRight size={14}/>Open Medousa</button>
      <button type="button" onclick={onHide}><X size={14}/>Hide companion</button>
    </div>
  {/if}
  {#if bodyOpen}
    {#if hasNotice}<div class="bar-notice">{@render notice?.()}</div>{/if}
    {#if model.restoreError && !conversation.target}<p class="bar-error" role="alert">{model.restoreError}</p>{/if}
    {#if actionError}<p class="bar-error" role="alert">{actionError}</p>{/if}
    {#if !picking}
      {#if conversation.target}
        <SpotlightTranscript {conversation} {onOpen}/>
      {:else}
        <div class="bar-results" id="companion-results" role="listbox" aria-label="Companion results" bind:this={resultList}>
          {#each results as result, i (result.id)}
            {#if i === 0 || results[i - 1].group !== result.group}<div class="result-group" role="presentation">{result.group}</div>{/if}
            {@const ResultIcon = result.kind === "action" ? actionIcons[result.action.icon] : result.kind === "ask" ? Sparkles : result.target.kind === "bot" ? Bot : result.target.kind === "new" ? Plus : MessageSquare}
            <button type="button" role="option" aria-selected={i === index} id={`companion-result-${i}`} data-result-index={i}
              class="result-row" disabled={!enabled || actionBusy} onpointermove={() => index = i} onclick={() => void choose(result)}>
              <ResultIcon size={15}/><span class="result-copy"><span>{result.label}</span><small>{result.description}</small></span>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
    <footer class="bar-footer"><span>{picking ? "↑↓ Choose · ↵ Select" : conversation.target ? "↵ Send · esc Close" : "↑↓ Navigate · ↵ Open"}</span><span>{scopeLabel}</span></footer>
  {/if}
</section>

<style>
  .companion-conversation { position: relative; display: flex; flex: 1; flex-direction: column; min-height: 0; min-width: 0; border: 1px solid rgb(var(--theme-border) / .45); border-radius: .85rem; background: rgb(var(--theme-pane)); color: rgb(var(--theme-text)); box-shadow: 0 10px 24px rgb(var(--theme-shadow) / .22); user-select: text; -webkit-user-select: text; }
  .companion-conversation.body-open { height: 100%; }
  .companion-conversation :global(.command-spotlight-search-row) { flex: 0 0 auto; min-height: 3.6rem; gap: .35rem; padding-right: .55rem; border-bottom: 0; }
  .companion-conversation :global(.command-spotlight-input) { min-width: 0; padding: .9rem 0 .9rem .9rem; font-size: .875rem; }
  .companion-conversation :global(.command-spotlight-recipient) { margin-left: .65rem; max-width: 38%; padding: .3rem .4rem; font-size: .8rem; }
  .companion-conversation :global(.command-spotlight-recipient + .command-spotlight-input) { padding-left: .15rem; }
  .companion-conversation :global(.command-spotlight-target-picker-inline) { position: static; flex: 1; min-height: 0; max-height: none; margin: 0; padding: .45rem; border: 0; border-top: 1px solid rgb(var(--theme-border) / .3); border-radius: 0; background: transparent; box-shadow: none; }
  .companion-conversation :global(.command-spotlight-section-label) { padding: .5rem .55rem .25rem; }
  .companion-conversation :global(.command-spotlight-row) { padding: .5rem; gap: .6rem; }
  .companion-conversation :global(.command-spotlight-row-title) { font-size: .82rem; }
  .companion-conversation :global(.command-spotlight-row-subtitle), .companion-conversation :global(.command-spotlight-row-meta) { font-size: .72rem; }
  .companion-conversation :global(.command-spotlight-conversation) { border-top: 1px solid rgb(var(--theme-border) / .3); padding: 1rem 1.1rem .8rem; }
  .companion-conversation :global(.command-spotlight-transcript-entry) { margin-bottom: 1.1rem; }
  .companion-conversation :global(.command-spotlight-transcript-entry p) { font-size: .83rem; line-height: 1.6; overflow-wrap: anywhere; }
  .companion-conversation :global(.command-spotlight-transcript-speaker), .companion-conversation :global(.command-spotlight-open-conversation), .companion-conversation :global(.command-spotlight-conversation-notice), .companion-conversation :global(.command-spotlight-conversation-empty) { font-size: .72rem; }
  .bar-icon { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 1.7rem; height: 1.8rem; padding: .25rem; border: 0; border-radius: .35rem; background: transparent; color: rgb(var(--theme-text-tertiary)); cursor: pointer; }
  .bar-icon:hover { color: rgb(var(--theme-text)); background: rgb(var(--theme-text) / .06); }
  .bar-icon:disabled { opacity: .4; cursor: default; }
  .bar-menu { position: absolute; z-index: 5; top: 3.2rem; right: .55rem; width: 11.5rem; padding: .35rem; border: 1px solid rgb(var(--theme-border) / .45); border-radius: .55rem; background: rgb(var(--theme-pane)); box-shadow: 0 10px 24px rgb(var(--theme-shadow) / .25); }
  .bar-menu button { display: flex; align-items: center; gap: .6rem; width: 100%; padding: .5rem .6rem; border: 0; border-radius: .35rem; background: transparent; color: rgb(var(--theme-text-secondary)); font-size: .76rem; text-align: left; cursor: pointer; }
  .bar-menu button:hover { background: rgb(var(--theme-text) / .06); color: rgb(var(--theme-text)); }
  .bar-menu button:disabled { opacity: .4; }
  .bar-results { flex: 1; min-height: 0; overflow-y: auto; padding: .45rem; border-top: 1px solid rgb(var(--theme-border) / .3); }
  .result-group { padding: .5rem .65rem .3rem; color: rgb(var(--theme-text-tertiary)); text-transform: uppercase; letter-spacing: .1em; font-size: .65rem; }
  .result-row { display: flex; align-items: center; gap: .65rem; width: 100%; padding: .6rem .65rem; border: 0; border-radius: .45rem; background: transparent; color: rgb(var(--theme-text-secondary)); text-align: left; cursor: pointer; }
  .result-row:hover, .result-row[aria-selected="true"] { background: rgb(var(--theme-text) / .06); color: rgb(var(--theme-text)); }
  .result-copy { display: flex; flex-direction: column; min-width: 0; gap: .1rem; }
  .result-copy > span { font-size: .82rem; }
  .result-copy small { color: rgb(var(--theme-text-tertiary)); font-size: .72rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar-footer { display: flex; justify-content: space-between; flex-shrink: 0; gap: .5rem; border-top: 1px solid rgb(var(--theme-border) / .3); padding: .6rem .9rem; color: rgb(var(--theme-text-tertiary)); font-size: .65rem; }
  .bar-footer span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar-error { flex-shrink: 0; padding: .6rem .9rem; margin: 0; color: rgb(var(--theme-text-secondary)); font-size: .75rem; }
  .bar-notice { flex-shrink: 0; padding: .5rem .9rem; }
</style>
