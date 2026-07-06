<script lang="ts">
  import {
    dismissChatAsyncToolsHint,
    isChatAsyncToolsHintDismissed,
  } from "$lib/config/chatTrustHints";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { isChatLaneMessage } from "$lib/utils/askThreads";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let dismissed = $state(isChatAsyncToolsHintDismissed());

  const chatMessages = $derived(chat.messages.filter((message) => isChatLaneMessage(message)));

  /** Last settled assistant turn has tool names in history but no structured receipts in UI. */
  const receiptGap = $derived.by(() => {
    const last = [...chatMessages].reverse().find((message) => message.role === "assistant");
    if (!last || last.streaming) return false;
    const namedTools = last.tools?.length ?? 0;
    const structuredRuns = last.toolRuns?.length ?? 0;
    return namedTools > 0 && structuredRuns === 0;
  });

  const visible = $derived(settings.showWorkshopGuidance && !dismissed);

  function hideHint() {
    dismissChatAsyncToolsHint();
    dismissed = true;
  }

  function openHistory() {
    automationsNav.openSection("history");
    layout.navigateDesktop("automations", { bump: true });
    if (mobile) layout.openMore("automations");
  }
</script>

{#if visible}
  <div class="chat-async-tools-hint" role="note">
    <p class="chat-async-tools-hint-copy">
      {#if receiptGap}
        Tools ran this turn but receipts may not appear in chat yet — common during reconnects
        or context updates.
      {:else}
        What you see in chat can lag behind tool execution. Activity shows live receipts;
        Automations → History keeps the full audit trail.
      {/if}
    </p>
    <div class="chat-async-tools-hint-actions">
      <button type="button" class="workshop-text-action text-[11px]" onclick={openHistory}>
        Open history
      </button>
      <button type="button" class="workshop-text-action text-[11px]" onclick={hideHint}>
        Dismiss
      </button>
    </div>
  </div>
{/if}

<style>
  .chat-async-tools-hint {
    margin: 0 0 0.75rem;
    padding: 0.55rem 0.7rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-400) 22%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
  }

  .chat-async-tools-hint-copy {
    margin: 0;
    font-size: 0.6875rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-300));
  }

  .chat-async-tools-hint-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
    margin-top: 0.35rem;
  }
</style>
