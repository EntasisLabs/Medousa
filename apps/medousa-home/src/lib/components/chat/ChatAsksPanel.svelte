<script lang="ts">
  import "$lib/styles/work.postcss";
  import { onDestroy } from "svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import {
    saveChatTurnToVault,
    showChatTurnSaveFeedback,
  } from "$lib/utils/saveChatTurnToVault";
  import { groupAskThreads } from "$lib/utils/askThreads";
  import { openNewAsk } from "$lib/utils/workChromeEvents";
  import { taskOverlays } from "$lib/runtime/taskOverlayState.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { focusChatComposer } from "$lib/runtime/shellUseCases";
  import type { ChatMessage } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  let alive = true;
  let moving = $state(false);
  let actionError = $state("");
  onDestroy(() => { alive = false; });

  let cardDetailOpen = $state(false);
  let cardDetail = $state<CardDetailPayload | null>(null);

  const askThreads = $derived(groupAskThreads(chat.messagesFor(sessionId)));

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
    taskOverlays.askHistorySessionId = null;
    if (layout.isMobile) layout.openMore("automations");
  }

  async function handleSaveToVault(assistant: ChatMessage, user?: ChatMessage | null) {
    const scope = activeWorkshopId();
    const result = await saveChatTurnToVault({
      assistant,
      user: user ?? null,
      sessionId,
    });
    if (alive && activeWorkshopId() === scope) showChatTurnSaveFeedback(result);
  }

  async function promoteToChat(jobId: string) {
    if (moving) return;
    const workshopId = activeWorkshopId();
    moving = true; actionError = "";
    try {
      if (chat.sessionId !== sessionId) await chat.switchSession(sessionId);
      if (!alive || activeWorkshopId() !== workshopId || chat.sessionId !== sessionId) return;
      chat.promoteAskToChat(jobId);
      taskOverlays.askHistorySessionId = null;
      focusChatComposer();
    } catch (cause) {
      if (alive && activeWorkshopId() === workshopId) actionError = cause instanceof Error ? cause.message : String(cause);
    } finally { if (alive) moving = false; }
  }

  function goNewAsk() { openNewAsk(); }

</script>

<div class="work-asks-panel flex h-full min-h-0 min-w-0 flex-1 flex-col">
  <header class="work-asks-header shrink-0">
    <div class="flex min-w-0 items-start gap-2">
      <div class="min-w-0">
        <h1 class="text-sm font-semibold text-surface-50">Asks</h1>
        <p class="mt-0.5 text-[11px] text-content-quiet">
          Background asks from this conversation
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

  {#if actionError}<p role="alert" class="px-4 py-2 text-xs text-content-error">{actionError}</p>{/if}
  <div class="work-asks-scroll min-h-0 flex-1 overflow-y-auto">
    {#if askThreads.length === 0}
      <div class="work-asks-empty">
        <p class="text-sm text-content-tertiary">No asks yet</p>
        <p class="mt-1 text-[12px] text-content-quiet">
          Start a new ask here, or use /ask in Chat.
        </p>
        <button
          type="button"
          class="workshop-text-action mt-3 text-[12px]"
          onclick={goNewAsk}
        >
          New ask
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
                <p class="mt-0.5 text-[10px] text-content-quiet">
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
                  disabled={moving}
                  onclick={() => void promoteToChat(thread.jobId)}
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
