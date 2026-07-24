<script lang="ts">
  import { ArrowLeft } from "@lucide/svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import {
    saveChatTurnToVault,
    showChatTurnSaveFeedback,
  } from "$lib/utils/saveChatTurnToVault";
  import { groupAskThreads } from "$lib/utils/askThreads";
  import { dispatchWorkFocusAsk } from "$lib/utils/workChromeEvents";
  import type { ChatMessage } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

  interface Props {
    onOpenChat?: () => void;
  }

  let { onOpenChat }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();
  let cardDetailOpen = $state(false);
  let cardDetail = $state<CardDetailPayload | null>(null);

  const askThreads = $derived(groupAskThreads(chat.messages));
  const sessionId = $derived(chat.sessionId);

  function openCardDetail(detail: CardDetailPayload) {
    cardDetail = detail;
    cardDetailOpen = true;
  }

  function closeCardDetail() {
    cardDetailOpen = false;
  }

  function handlePromoteToFlow(ref: ToolHistorySliceRef) {
    flowDraft.queuePromotion([ref]);
    automationsNav.openSection("flows");
    layout.navigateDesktop("automations", { bump: true });
    if (layout.isMobile) layout.openMore("automations");
  }

  async function handleSaveToVault(assistant: ChatMessage, user?: ChatMessage | null) {
    const result = await saveChatTurnToVault({
      assistant,
      user: user ?? null,
      sessionId,
    });
    showChatTurnSaveFeedback(result);
  }

  function promoteToChat(jobId: string) {
    chat.promoteAskToChat(jobId);
    onOpenChat?.();
  }

  function goNewAsk() {
    workspace.openHubView();
    dispatchWorkFocusAsk();
  }
</script>

<div class="work-asks-panel flex h-full min-h-0 min-w-0 flex-1 flex-col">
  <header class="work-asks-header shrink-0">
    <div class="flex min-w-0 items-start gap-2">
      {#if layout.isMobile}
        <button
          type="button"
          class="mobile-icon-btn shrink-0"
          aria-label="Back to Home"
          onclick={() => workspace.openHubView()}
        >
          <ArrowLeft size={18} strokeWidth={1.85} />
        </button>
      {/if}
      <div class="min-w-0">
        <h1 class="text-sm font-semibold text-surface-50">Asks</h1>
        <p class="mt-0.5 text-[11px] text-surface-500">
          Scoped background work — separate from Chat
        </p>
      </div>
    </div>
    <button
      type="button"
      class="workshop-text-action shrink-0 text-[11px]"
      onclick={goNewAsk}
    >
      New ask
    </button>
  </header>

  <div bind:this={scrollEl} class="work-asks-scroll min-h-0 flex-1 overflow-y-auto">
    {#if askThreads.length === 0}
      <div class="work-asks-empty">
        <p class="text-sm text-surface-400">No asks yet</p>
        <p class="mt-1 text-[12px] text-surface-500">
          Start one from the Board, or use /ask in Chat.
        </p>
        <button
          type="button"
          class="workshop-text-action mt-3 text-[12px]"
          onclick={goNewAsk}
        >
          New ask on Board
        </button>
      </div>
    {:else}
      <section class="chat-ask-rail space-y-3 px-4 py-3">
        {#each askThreads as thread (thread.jobId)}
          <article class="chat-ask-thread">
            <header class="chat-ask-thread-header">
              <div class="min-w-0">
                <p class="truncate text-sm font-medium text-surface-100">
                  {thread.promptPreview}
                </p>
                <p class="mt-0.5 text-[10px] text-surface-500">
                  {#if thread.active}
                    In progress
                  {:else}
                    Settled
                  {/if}
                </p>
              </div>
              {#if !thread.active}
                <button
                  type="button"
                  class="workshop-text-action shrink-0 text-[11px]"
                  onclick={() => promoteToChat(thread.jobId)}
                >
                  Move to chat
                </button>
              {/if}
            </header>
            <div class="chat-ask-thread-body space-y-3">
              <ChatMessageList
                messages={thread.messages}
                {sessionId}
                mobile={layout.isMobile}
                compact={true}
                scrollRoot={scrollEl}
                onPromoteToFlow={handlePromoteToFlow}
                onSaveToVault={handleSaveToVault}
                onOpenCardDetail={openCardDetail}
              />
            </div>
          </article>
        {/each}
      </section>
    {/if}
  </div>
</div>

<LiquidCardDetailSheet
  open={cardDetailOpen}
  detail={cardDetail}
  onClose={closeCardDetail}
/>

<style>
  .work-asks-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.75rem 1rem 0.5rem;
    border-bottom: 1px solid rgb(var(--color-surface-700) / 0.35);
  }

  .work-asks-empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 2rem 1.25rem;
  }
</style>
