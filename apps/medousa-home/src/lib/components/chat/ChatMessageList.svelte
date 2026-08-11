<script lang="ts">
  /**
   * Chat message list — runtime-governed Liquid is the sole paint path.
   * User→assistant pairs render as timeline beats (whisper + full-width voice).
   */
  import { Copy, Library, Share2 } from "@lucide/svelte";
  import ChatSubagentRow from "$lib/components/chat/ChatSubagentRow.svelte";
  import ChatUserWhisper from "$lib/components/chat/ChatUserWhisper.svelte";
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import type { SubagentRow } from "$lib/utils/subagentRows";
  import { chat } from "$lib/stores/chat.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { shareText } from "$lib/share";
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
  import { copyTextToClipboard } from "$lib/utils/vaultClipboard";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";

  interface Props {
    messages: ChatMessage[];
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    /** When true, collapse worker handoff+synthesis into one visual turn. */
    workerThread?: boolean;
    /** Stamp main-thread turn wrappers for the conversation navigator. */
    navigation?: boolean;
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
    /** Sub-agent beats keyed by work id, anchored on their worker-lane message. */
    subagentRows?: Map<string, SubagentRow>;
    onOpenSubagent?: (workId: string) => void;
    onStopSubagent?: (workId: string) => void;
  }

  let {
    messages,
    sessionId,
    mobile = false,
    compact = false,
    workerThread = false,
    navigation = false,
    scrollRoot = null,
    onPromoteToFlow,
    onSubmitIntent,
    onSaveToVault,
    onOpenCardDetail,
    subagentRows,
    onOpenSubagent,
    onStopSubagent,
  }: Props = $props();

  /** Worker-lane turns carry the sub-agent beat that produced them. */
  function subagentFor(message: ChatMessage): SubagentRow | null {
    if (message.lane !== "worker") return null;
    const workId = message.workId?.trim();
    if (!workId) return null;
    return subagentRows?.get(workId) ?? null;
  }

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

  function canCopyAssistantTurn(message: ChatMessage): boolean {
    return canSaveAssistantTurn(message);
  }

  function saveAssistant(assistant: ChatMessage, user: ChatMessage | null = null) {
    if (!onSaveToVault || !canSaveAssistantTurn(assistant)) return;
    void onSaveToVault(assistant, user);
  }

  async function copyAssistant(assistant: ChatMessage) {
    const raw = assistant.content ?? "";
    if (!raw.trim()) return;
    const ok = await copyTextToClipboard(raw);
    toast.show(ok ? "Copied" : "Couldn’t copy", { durationMs: 1400 });
  }

  async function shareAssistant(assistant: ChatMessage) {
    const raw = assistant.content ?? "";
    if (!raw.trim()) return;
    const outcome = await shareText("Medousa reply", raw);
    if (outcome === "shared") {
      toast.show("Shared", { durationMs: 1400 });
    } else if (outcome === "copied") {
      toast.show("Copied to clipboard", { durationMs: 1600 });
    } else {
      toast.show("Couldn’t share", { durationMs: 1600 });
    }
  }
</script>

{#snippet turnActions(assistant: ChatMessage, user: ChatMessage | null = null)}
  {@const showCopy = canCopyAssistantTurn(assistant)}
  {@const showShare = canCopyAssistantTurn(assistant)}
  {@const showSave = onSaveToVault && canSaveAssistantTurn(assistant)}
  {#if assistant.responseModel || showCopy || showShare || showSave}
    <div class="chat-turn-actions" class:chat-turn-actions--mobile={mobile}>
      {#if assistant.responseModel}
        <span
          class="chat-model-receipt"
          title={`Successful inference route reported by Medousa: ${assistant.responseProvider ?? "provider"} · ${assistant.responseModel}`}
        >
          {formatModelDisplayName(assistant.responseModel, 28)}
          {#if assistant.responseProvider === "openai-codex"}
            <span aria-hidden="true">·</span> ChatGPT
          {/if}
        </span>
      {/if}
      {#if showCopy}
        <button
          type="button"
          class="chat-turn-action"
          title="Copy"
          aria-label="Copy"
          onclick={() => void copyAssistant(assistant)}
        >
          <Copy size={14} strokeWidth={1.75} />
        </button>
      {/if}
      {#if showShare}
        <button
          type="button"
          class="chat-turn-action"
          title="Share"
          aria-label="Share"
          onclick={() => void shareAssistant(assistant)}
        >
          <Share2 size={14} strokeWidth={1.75} />
        </button>
      {/if}
      {#if showSave}
        <button
          type="button"
          class="chat-turn-action"
          title="Save to Library"
          aria-label="Save to Library"
          onclick={() => saveAssistant(assistant, user)}
        >
          <Library size={14} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet subagentBeat(row: SubagentRow)}
  <ChatSubagentRow
    {row}
    {compact}
    onOpen={() => onOpenSubagent?.(row.workId)}
    onStop={onStopSubagent ? () => onStopSubagent(row.workId) : undefined}
  />
{/snippet}

{#each beats as beat, beatIndex (beat.kind === "pair" ? `${beat.user.id}:${beat.assistant.id}` : beat.message.id)}
  {@const previousBeat = beatIndex > 0 ? beats[beatIndex - 1] : null}
  {@const turnBreak =
    previousBeat != null &&
    (previousBeat.kind === "pair" || previousBeat.message.role === "assistant") &&
    (beat.kind === "pair" || beat.message.role === "user")}

  {#if beat.kind === "pair"}
    <section
      class="chat-turn-beat {turnBreak ? 'chat-turn-break' : ''}"
      data-chat-turn-user-id={navigation ? beat.user.id : undefined}
    >
      <ChatUserWhisper
        message={beat.user}
        {sessionId}
        {mobile}
        {compact}
        {scrollRoot}
        forceExpand={shouldForceExpandUserWhisper(painted, beat.user.id)}
        {onSubmitIntent}
      />
      {#if subagentFor(beat.assistant)}
        {@render subagentBeat(subagentFor(beat.assistant)!)}
      {/if}
      <article class="group relative {assistantClass(beat.assistant)}">
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
        {@render turnActions(beat.assistant, beat.user)}
      </article>
    </section>
  {:else if beat.message.role === "user"}
    <div
      class="{turnBreak ? 'chat-turn-break' : ''} chat-turn-beat"
      data-chat-turn-user-id={navigation ? beat.message.id : undefined}
    >
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
    {#if subagentFor(beat.message)}
      {@render subagentBeat(subagentFor(beat.message)!)}
    {/if}
    <article class="group relative {turnBreak ? 'chat-turn-break' : ''} {assistantClass(beat.message)}">
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
      {#if beat.message.role === "assistant"}
        {@render turnActions(beat.message, null)}
      {/if}
    </article>
  {/if}
{/each}
