<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { resumeBrowserChallenge } from "$lib/utils/resumeBrowserChallenge";

  interface Props {
    compact?: boolean;
    onContinue?: () => void;
  }

  let { compact = false, onContinue }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  const pending = $derived(chat.browserChallenge);

  async function continueAgent() {
    if (!pending || busy || humanBrowser.loading) return;
    busy = true;
    error = null;
    try {
      await resumeBrowserChallenge(pending.sessionId);
      chat.clearBrowserChallenge(pending.sessionId);
      await browser.setControl("agent");
      onContinue?.();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if pending || browser.control === "awaiting_operator"}
  <div
    class="{compact
      ? 'flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-500/25 bg-amber-950/30 px-2 py-1 text-[11px] text-amber-100'
      : 'border-b border-amber-500/30 bg-amber-950/40 px-3 py-2 text-xs text-amber-100'}"
  >
    <span class="min-w-0 flex-1">
      {pending?.message?.trim() || "Medousa needs help with a web verification in this tab."}
    </span>
    {#if pending}
      <button
        type="button"
        class="btn btn-xs variant-filled-primary shrink-0"
        disabled={busy || humanBrowser.loading}
        onclick={() => void continueAgent()}
      >
        Continue agent
      </button>
    {/if}
    {#if error}
      <span class="w-full text-[10px] text-amber-200/80">{error}</span>
    {/if}
  </div>
{/if}
