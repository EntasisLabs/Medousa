<script lang="ts">
  import { X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    vaultContextHasSelection,
    vaultContextScopeHint,
    vaultContextScopeLabel,
  } from "$lib/utils/vaultNoteBridge";

  interface Props {
    compact?: boolean;
    /** Soft chip for sticky bottom-sheet chat. */
    whisper?: boolean;
    class?: string;
  }

  let { compact = false, whisper = false, class: className = "" }: Props = $props();

  const scope = $derived(chat.vaultNoteContext);
  const hint = $derived(scope ? vaultContextScopeHint(scope) : null);
  const label = $derived(scope ? vaultContextScopeLabel(scope) : "Note");
  const hasPassage = $derived(vaultContextHasSelection(scope));
</script>

{#if scope && hint}
  <div
    class="vault-chat-context-chip flex items-center gap-2 text-xs {whisper
      ? 'vault-chat-context-chip--whisper'
      : 'rounded-lg border border-primary-500/30 bg-primary-500/10 px-3 py-1.5 text-primary-100'} {className}"
    title={hasPassage ? scope.selection?.text : hint}
  >
    {#if !whisper}
      <span class="shrink-0 rounded bg-primary-500/25 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-primary-200">
        {label}
      </span>
    {/if}
    <span class="min-w-0 truncate {whisper ? 'text-[11px] text-surface-400' : 'font-medium'}">
      {#if whisper}
        {hasPassage ? `Passage in “${scope.title}”` : `On “${scope.title}”`}
      {:else if hasPassage}
        {scope.title}
        <span class="font-normal text-primary-200/75"> · {hint}</span>
      {:else}
        {scope.title}
      {/if}
    </span>
    {#if !compact && !whisper && !hasPassage}
      <span class="shrink-0 text-primary-200/75">· {hint}</span>
    {/if}
    <button
      type="button"
      class="ml-auto shrink-0 rounded p-0.5 transition {whisper
        ? 'text-surface-500 hover:bg-surface-800 hover:text-surface-200'
        : 'text-primary-200/80 hover:bg-primary-500/20 hover:text-primary-50'}"
      aria-label={hasPassage ? "Clear passage context" : "Clear note context"}
      onclick={() => chat.clearVaultNoteContext()}
    >
      <X size={whisper ? 12 : 14} strokeWidth={2} />
    </button>
  </div>
{/if}
