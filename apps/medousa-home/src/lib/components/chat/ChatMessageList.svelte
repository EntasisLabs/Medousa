<script lang="ts">
  /**
   * Chat message list — runtime-governed Liquid is the sole paint path.
   * Empty worker/workshop handoff shells are filtered so one turn = one surface.
   */
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ChatMessage } from "$lib/types/chat";
  import {
    presentChatMessages,
    presentWorkerThreadMessages,
  } from "$lib/utils/presentChatTurns";

  interface Props {
    messages: ChatMessage[];
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    /** When true, collapse worker handoff+synthesis into one visual turn. */
    workerThread?: boolean;
    onPromoteToFlow?: (
      ref: import("$lib/types/toolHistory").ToolHistorySliceRef,
    ) => void | Promise<void>;
    /** Spawn a new interactive turn from a scene interaction (action_row / button). */
    onSubmitIntent?: (text: string) => void;
  }

  let {
    messages,
    sessionId,
    mobile = false,
    compact = false,
    workerThread = false,
    onPromoteToFlow,
    onSubmitIntent,
  }: Props = $props();

  const painted = $derived(
    workerThread ? presentWorkerThreadMessages(messages) : presentChatMessages(messages),
  );

  function retryWorkerSynthesis(workId: string | null | undefined) {
    const trimmed = workId?.trim();
    if (!trimmed) return;
    void chat.retryWorkerSynthesis(trimmed);
  }
</script>

{#each painted as message, index (message.id)}
  {@const previous = index > 0 ? painted[index - 1] : null}
  {@const turnBreak = message.role === "user" && previous?.role === "assistant"}
  {#if mobile && message.role === "user"}
    <div class="{turnBreak ? 'chat-turn-break' : ''} mobile-chat-user-row">
      <article class="mobile-chat-bubble-user">
        <LiquidChatMessage {message} {sessionId} {mobile} compact {onSubmitIntent} />
      </article>
    </div>
  {:else}
    <article
      class="{turnBreak ? 'chat-turn-break' : ''} {mobile && message.role === 'assistant'
        ? 'mobile-chat-bubble-assistant'
        : ''} {message.role === 'user'
        ? compact
          ? 'chat-user-bubble chat-user-bubble-compact'
          : 'chat-user-bubble'
        : message.role === 'system'
          ? 'workshop-faint px-1'
          : mobile
            ? ''
            : compact
              ? 'chat-voice chat-voice-compact'
              : 'chat-voice'}"
    >
      <LiquidChatMessage
        {message}
        {sessionId}
        {mobile}
        {compact}
        {onPromoteToFlow}
        {onSubmitIntent}
        onRetryWorker={retryWorkerSynthesis}
      />
    </article>
  {/if}
{/each}
