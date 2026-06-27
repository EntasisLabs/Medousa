<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { resumeBrowserSession } from "$lib/daemon";

  interface Props {
    onContinue?: () => void;
  }

  let { onContinue }: Props = $props();

  let busy = $state(false);

  const pending = $derived(chat.browserChallenge);

  async function continueAgent() {
    if (!pending || busy) return;
    busy = true;
    try {
      await resumeBrowserSession(pending.sessionId);
      chat.clearBrowserChallenge(pending.sessionId);
      await browser.setControl("agent");
      onContinue?.();
    } catch {
      // Keep banner visible; workshop panel shows error details.
    } finally {
      busy = false;
    }
  }
</script>

{#if pending || browser.control === "awaiting_operator"}
  <div class="border-b border-amber-500/30 bg-amber-950/40 px-3 py-2 text-xs text-amber-100">
    <div class="flex flex-wrap items-center gap-2">
      <span>Medousa needs help with a web verification in this tab.</span>
      {#if pending}
        <button
          type="button"
          class="btn btn-xs variant-filled-primary"
          disabled={busy}
          onclick={() => void continueAgent()}
        >
          Continue agent
        </button>
      {/if}
    </div>
  </div>
{/if}
