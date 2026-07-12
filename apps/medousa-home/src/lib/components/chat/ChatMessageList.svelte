<script lang="ts">
  /**
   * Chat message list — runtime-governed Liquid is the sole paint path.
   * User→assistant pairs render as timeline beats (whisper + full-width voice).
   */
  import ChatUserWhisper from "$lib/components/chat/ChatUserWhisper.svelte";
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ChatMessage } from "$lib/types/chat";
  import {
    groupChatTurnBeats,
    shouldForceExpandUserWhisper,
  } from "$lib/utils/chatTurnBeats";
  import { canSaveAssistantTurn } from "$lib/utils/saveChatTurnToVault";
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
    /** Scroll container for user-whisper IntersectionObserver. */
    scrollRoot?: HTMLElement | null;
    onPromoteToFlow?: (
      ref: import("$lib/types/toolHistory").ToolHistorySliceRef,
    ) => void | Promise<void>;
    /** Spawn a new interactive turn from a scene interaction (action_row / button). */
    onSubmitIntent?: (text: string) => void;
    /** Promote settled assistant markdown to a Library inbox note. */
    onSaveToVault?: (assistant: ChatMessage, user?: ChatMessage | null) => void | Promise<void>;
    /** Open structured card detail sheet (Monogram expand). */
    onOpenCardDetail?: (detail: import("$lib/markdown/liquidEmbeds").CardDetailPayload) => void;
  }

  let {
    messages,
    sessionId,
    mobile = false,
    compact = false,
    workerThread = false,
    scrollRoot = null,
    onPromoteToFlow,
    onSubmitIntent,
    onSaveToVault,
    onOpenCardDetail,
  }: Props = $props();

  const painted = $derived(
    workerThread ? presentWorkerThreadMessages(messages) : presentChatMessages(messages),
  );
  const beats = $derived(groupChatTurnBeats(painted));

  function retryWorkerSynthesis(workId: string | null | undefined) {
    const trimmed = workId?.trim();
    if (!trimmed) return;
    void chat.retryWorkerSynthesis(trimmed);
  }

  function assistantClass(message: ChatMessage): string {
    if (message.role === "system") return "workshop-faint px-1";
    if (message.role === "user") return "";
    const voice = compact ? "chat-voice chat-voice-full chat-voice-compact" : "chat-voice chat-voice-full";
    if (mobile) return `mobile-chat-voice-full ${voice}`;
    return voice;
  }

  function saveAssistant(assistant: ChatMessage, user: ChatMessage | null = null) {
    if (!onSaveToVault || !canSaveAssistantTurn(assistant)) return;
    void onSaveToVault(assistant, user);
  }
</script>

{#each beats as beat, beatIndex (beat.kind === "pair" ? `${beat.user.id}:${beat.assistant.id}` : beat.message.id)}
  {@const previousBeat = beatIndex > 0 ? beats[beatIndex - 1] : null}
  {@const turnBreak =
    previousBeat != null &&
    (previousBeat.kind === "pair" || previousBeat.message.role === "assistant") &&
    (beat.kind === "pair" || beat.message.role === "user")}

  {#if beat.kind === "pair"}
    <section class="chat-turn-beat {turnBreak ? 'chat-turn-break' : ''}">
      <ChatUserWhisper
        message={beat.user}
        {sessionId}
        {mobile}
        {compact}
        {scrollRoot}
        forceExpand={shouldForceExpandUserWhisper(painted, beat.user.id)}
        {onSubmitIntent}
      />
      <article class="group relative {assistantClass(beat.assistant)}">
        {#if onSaveToVault && canSaveAssistantTurn(beat.assistant)}
          <button
            type="button"
            class="chat-save-to-library workshop-text-action"
            onclick={() => saveAssistant(beat.assistant, beat.user)}
          >
            Save to Library
          </button>
        {/if}
        <LiquidChatMessage
          message={beat.assistant}
          {sessionId}
          {mobile}
          {compact}
          {onPromoteToFlow}
          {onSubmitIntent}
          {onOpenCardDetail}
          onRetryWorker={retryWorkerSynthesis}
        />
      </article>
    </section>
  {:else if beat.message.role === "user"}
    <div class="{turnBreak ? 'chat-turn-break' : ''} chat-turn-beat">
      <ChatUserWhisper
        message={beat.message}
        {sessionId}
        {mobile}
        {compact}
        {scrollRoot}
        forceExpand={shouldForceExpandUserWhisper(painted, beat.message.id)}
        {onSubmitIntent}
      />
    </div>
  {:else}
    <article class="group relative {turnBreak ? 'chat-turn-break' : ''} {assistantClass(beat.message)}">
      {#if beat.message.role === "assistant" && onSaveToVault && canSaveAssistantTurn(beat.message)}
        <button
          type="button"
          class="chat-save-to-library workshop-text-action"
          onclick={() => saveAssistant(beat.message, null)}
        >
          Save to Library
        </button>
      {/if}
      <LiquidChatMessage
        message={beat.message}
        {sessionId}
        {mobile}
        {compact}
        {onPromoteToFlow}
        {onSubmitIntent}
        {onOpenCardDetail}
        onRetryWorker={retryWorkerSynthesis}
      />
    </article>
  {/if}
{/each}
