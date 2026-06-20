<script lang="ts">
  import { X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { scriptWorkbenchContextHint } from "$lib/utils/scriptWorkbenchBridge";

  interface Props {
    compact?: boolean;
    class?: string;
  }

  let { compact = false, class: className = "" }: Props = $props();

  const scope = $derived(chat.scriptWorkbenchContext);
  const hint = $derived(scope ? scriptWorkbenchContextHint(scope) : null);
</script>

{#if scope && hint}
  <div
    class="vault-chat-context-chip flex items-center gap-2 rounded-lg border border-primary-500/30 bg-primary-500/10 px-3 py-1.5 text-xs text-primary-100 {className}"
    title={hint}
  >
    <span class="shrink-0 rounded bg-primary-500/25 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-primary-200">
      Script
    </span>
    <span class="min-w-0 truncate font-medium">{scope.name}</span>
    {#if !compact}
      <span class="shrink-0 text-primary-200/75">· {hint}</span>
    {/if}
    <button
      type="button"
      class="ml-auto shrink-0 rounded p-0.5 text-primary-200/80 transition hover:bg-primary-500/20 hover:text-primary-50"
      aria-label="Clear script context"
      onclick={() => chat.clearScriptWorkbenchContext()}
    >
      <X size={14} strokeWidth={2} />
    </button>
  </div>
{/if}
