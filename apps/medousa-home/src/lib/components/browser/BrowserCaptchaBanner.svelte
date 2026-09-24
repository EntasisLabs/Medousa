<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();
  const pending = $derived(chat.browserChallenge);
</script>

{#if !pending && browser.control === "awaiting_operator"}
  <div
    class="{compact
      ? 'flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-500/25 bg-amber-950/30 px-2 py-1 text-[11px] text-amber-100'
      : 'border-b border-amber-500/30 bg-amber-950/40 px-3 py-2 text-xs text-amber-100'}"
  >
    <span class="min-w-0 flex-1">Medousa needs help with a web verification in this tab.</span>
  </div>
{/if}
