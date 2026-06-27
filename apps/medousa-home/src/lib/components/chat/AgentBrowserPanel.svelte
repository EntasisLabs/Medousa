<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { haptic } from "$lib/haptics";
  import { openInBrowser } from "$lib/utils/openInBrowser";
  import { resumeBrowserChallenge } from "$lib/utils/resumeBrowserChallenge";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let busy = $state(false);
  let feedback = $state<string | null>(null);

  const pending = $derived(chat.browserChallenge);

  async function openWebVerification() {
    if (!pending?.challengeUrl) return;
    await openInBrowser(pending.challengeUrl, {
      openedBy: "agent",
      sessionId: chat.sessionId,
      openWorkshop: true,
    });
  }

  async function continueAgent() {
    if (!pending || busy || humanBrowser.loading) return;
    busy = true;
    feedback = null;
    try {
      await resumeBrowserChallenge(pending.sessionId);
      chat.clearBrowserChallenge(pending.sessionId);
      await browser.setControl("agent");
      feedback = "Verification submitted — agent will continue.";
      haptic("success");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if pending}
  <div
    class="{mobile
      ? 'mx-3 mb-2 rounded-xl border border-primary-500/30 bg-surface-900/80 p-3'
      : 'mx-4 mb-2 rounded-lg border border-primary-500/25 bg-surface-900/70 px-3 py-2.5'}"
    role="region"
    aria-label="Agent browser verification"
  >
    <div class="flex flex-col gap-2">
      <div>
        <p class="text-xs font-medium text-primary-200">Medousa needs help with a verification</p>
        <p class="mt-0.5 text-sm text-surface-100">
          {pending.message || "Complete the check in the Web browser, then continue the agent."}
        </p>
        {#if pending.challengeUrl}
          <p class="workshop-faint mt-1 truncate text-xs" title={pending.challengeUrl}>
            {pending.challengeUrl}
          </p>
        {/if}
      </div>
      <div class="flex flex-wrap items-center gap-2">
        {#if pending.challengeUrl}
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={() => void openWebVerification()}
          >
            Open in Web
          </button>
        {/if}
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={busy || humanBrowser.loading}
          onclick={() => void continueAgent()}
        >
          Continue agent
        </button>
        {#if feedback}
          <p class="text-xs text-surface-400">{feedback}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}
