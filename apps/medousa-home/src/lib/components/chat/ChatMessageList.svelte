<script lang="ts">
  import "$lib/styles/chat.postcss";
  /**
   * Chat message list — runtime-governed Liquid is the sole paint path.
   * User→assistant pairs render as timeline beats (whisper + full-width voice).
   */
  import VirtualChatBeat from "./VirtualChatBeat.svelte";
  import { onMount } from "svelte";
  import { Copy, Library, Share2, Square, Volume2 } from "@lucide/svelte";
  import ChatTaskGroup from "$lib/components/chat/ChatTaskGroup.svelte";
  import {
    joinAgentGroups,
    resolveAgentPlacementGroups,
  } from "$lib/utils/chatAgentGroups";
  import type { AgentPlacementGroup } from "$lib/types/generated/app_api";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import type { PeerProposalControls } from "./peerProposalControls";
  import ChatForkMenu from "$lib/components/chat/ChatForkMenu.svelte";
  import ChatUserWhisper from "$lib/components/chat/ChatUserWhisper.svelte";
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import type { SubagentRow } from "$lib/utils/subagentRows";
  import { chat } from "$lib/stores/chat.svelte";
  import { toast } from "$lib/runtime/toast.svelte";
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
  import { narration } from "$lib/stores/narration.svelte";

  interface Props {
    messages: ChatMessage[];
    sessionId: string;
    authorityId?: string | null;
    mobile?: boolean;
    compact?: boolean;
    /** When true, collapse worker handoff+synthesis into one visual turn. */
    workerThread?: boolean;
    /** Stamp main-thread turn wrappers for the conversation navigator. */
    navigation?: boolean;
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
    peerProposals?: PeerProposalReviewRecord[];
    peerControls?: PeerProposalControls;
    onOpenSubagent?: (workId: string) => void;
    onStopSubagent?: (workId: string) => void;
  }

  let {
    messages,
    sessionId,
    authorityId,
    mobile = false,
    compact = false,
    workerThread = false,
    navigation = false,
    onPromoteToFlow,
    onSubmitIntent,
    onSaveToVault,
    onOpenCardDetail,
    subagentRows,
    peerProposals = [],
    peerControls,
    onOpenSubagent,
    onStopSubagent,
  }: Props = $props();

  // Sync paint stays local (beats / worker-panel collapse). Placement visible
  // ids are projected through Rust so core owns the painted-id contract.
  const painted = $derived(
    workerThread ? presentWorkerThreadMessages(messages) : presentChatMessages(messages, subagentRows),
  );
  const beats = $derived(groupChatTurnBeats(painted));
  // Shared resolveAgentPlacementGroups owns ApplyVisibleItems + Project* fallback.
  let projectedGroups = $state<AgentPlacementGroup[]>([]);
  let placementEpoch = 0;
  $effect(() => {
    const token = ++placementEpoch;
    const workers = subagentRows ?? new Map();
    void resolveAgentPlacementGroups({
      messages,
      painted,
      proposals: peerProposals ?? [],
      workers,
      sessionId,
      authorityId,
    })
      .then((groups) => {
        if (token === placementEpoch) projectedGroups = groups;
      })
      .catch(() => {});
  });
  const agentGroups = $derived(joinAgentGroups(projectedGroups, peerProposals, subagentRows ?? new Map()));
  function delegationNodes(message: ChatMessage): string[] {
    return [...agentGroups.values()].flatMap(group => group.entries?.flatMap(entry =>
      entry.nodeId?.startsWith(`${message.id}:`) ? [entry.nodeId] : []) ?? []);
  }
  const groupPrefix = $props.id();
  function groupId(anchor: string | null) { return `${groupPrefix}-agents-${encodeURIComponent(anchor ?? "earlier")}`; }
  let forkingEntryId = $state<string | null>(null);

  onMount(() => narration.initialize());

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

  function canCarryDraft(message: ChatMessage): boolean {
    return message.transcript?.sessionId === chat.sessionId && Boolean(chat.draft.trim());
  }

  async function forkFrom(message: ChatMessage, includeDraft: boolean) {
    const entryId = message.transcript?.entryId;
    if (!entryId || forkingEntryId) return;
    forkingEntryId = entryId;
    try {
      await chat.forkFromEntry(message, { includeDraft });
      toast.show(includeDraft ? "Forked with draft" : "Conversation forked", {
        durationMs: 1700,
      });
    } catch (err) {
      toast.show(err instanceof Error ? err.message : "Couldn’t fork conversation", {
        durationMs: 2600,
      });
    } finally {
      forkingEntryId = null;
    }
  }
</script>

{#snippet turnActions(assistant: ChatMessage, user: ChatMessage | null = null)}
  {@const showCopy = canCopyAssistantTurn(assistant)}
  {@const showShare = canCopyAssistantTurn(assistant)}
  {@const showSave = onSaveToVault && canSaveAssistantTurn(assistant)}
  {@const showFork = Boolean(assistant.transcript?.entryId)}
  {@const showNarrate = narration.available && canCopyAssistantTurn(assistant)}
  {#if assistant.responseModel || showNarrate || showCopy || showShare || showSave || showFork}
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
      {#if showNarrate}
        <button
          type="button"
          class="chat-turn-action"
          title={narration.activeMessageId === assistant.id ? "Stop reading" : "Read aloud"}
          aria-label={narration.activeMessageId === assistant.id ? "Stop reading" : "Read aloud"}
          aria-pressed={narration.activeMessageId === assistant.id}
          onclick={() => narration.toggleMessage(assistant.id, assistant.content ?? "")}
        >
          {#if narration.activeMessageId === assistant.id}
            <Square size={11} strokeWidth={2} fill="currentColor" />
          {:else}
            <Volume2 size={14} strokeWidth={1.75} />
          {/if}
        </button>
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
      {#if showFork}
        <ChatForkMenu
          hasDraft={canCarryDraft(assistant)}
          busy={forkingEntryId === assistant.transcript?.entryId}
          {mobile}
          onFork={(includeDraft) => forkFrom(assistant, includeDraft)}
        />
      {/if}
    </div>
  {/if}
  {#if assistant.liveTranscripts?.length}
    <details class="mt-2 rounded-xl border border-surface-700/40 px-3 py-2 text-sm">
      <summary class="cursor-pointer text-surface-400">🎙 Live transcript</summary>
      <div class="mt-3 flex flex-col gap-3">
        {#each assistant.liveTranscripts as attachment (attachment.id)}
          {#each attachment.rows as row}
            <p class="whitespace-pre-wrap"><span class="text-surface-400">{row.role === "user" ? "You" : "Medousa"}:</span> {row.text}</p>
          {/each}
        {/each}
      </div>
    </details>
  {/if}
{/snippet}

{#snippet agentGroup(anchor: string | null, nodeId?: string)}
  {@const original = agentGroups.get(anchor)}
  {@const entries = original?.entries?.filter(entry => entry.nodeId === nodeId)}
  {#if entries?.length}
    {@const group = { entries, proposals: entries.flatMap(entry => entry.kind === 'proposal' ? [entry.row] : []), workers: entries.flatMap(entry => entry.kind === 'worker' ? [entry.row] : []) }}
    <ChatTaskGroup id={groupId(nodeId ?? anchor)} {group} controls={peerControls} {onOpenSubagent} {onStopSubagent} />
  {/if}
{/snippet}

{#snippet inlineAgents(nodeId: string)}
  {#each [...agentGroups.keys()] as anchor (anchor)}
    {@render agentGroup(anchor, nodeId)}
  {/each}
{/snippet}

{@render agentGroup(null)}

{#each beats as beat, beatIndex (beat.kind === "pair" ? beat.user.id : beat.message.id)}
  {@const previousBeat = beatIndex > 0 ? beats[beatIndex - 1] : null}
  {@const turnBreak =
    previousBeat != null &&
    (previousBeat.kind === "pair" || previousBeat.message.role === "assistant") &&
    (beat.kind === "pair" || beat.message.role === "user")}

  <VirtualChatBeat
    userId={navigation ? (beat.kind === "pair" ? beat.user.id : beat.message.role === "user" ? beat.message.id : undefined) : undefined}
    keepMounted={beat.kind === "pair" ? Boolean(beat.assistant.streaming) : Boolean(beat.message.streaming)}
  >
  {#if beat.kind === "pair"}
    <section
      class="chat-turn-beat {turnBreak ? 'chat-turn-break' : ''}"
    >
      <ChatUserWhisper
        message={beat.user}
        reactions={beat.assistant.reactions}
        {sessionId}
        {mobile}
        {compact}
        forceExpand={shouldForceExpandUserWhisper(painted, beat.user.id)}
        {onSubmitIntent}
        onFork={beat.user.transcript?.entryId
          ? (includeDraft) => forkFrom(beat.user, includeDraft)
          : undefined}
        forkBusy={forkingEntryId === beat.user.transcript?.entryId}
        forkHasDraft={canCarryDraft(beat.user)}
      />
    </section>
  {:else if beat.message.role === "user"}
    <div
      class="{turnBreak ? 'chat-turn-break' : ''} chat-turn-beat"
    >
      <ChatUserWhisper
        message={beat.message}
        {sessionId}
        {mobile}
        {compact}
        forceExpand={shouldForceExpandUserWhisper(painted, beat.message.id)}
        {onSubmitIntent}
        onFork={beat.message.transcript?.entryId
          ? (includeDraft) => forkFrom(beat.message, includeDraft)
          : undefined}
        forkBusy={forkingEntryId === beat.message.transcript?.entryId}
        forkHasDraft={canCarryDraft(beat.message)}
      />
    </div>
  {:else}
    {@render agentGroup(beat.message.id)}
    <article
      class="group relative {turnBreak ? 'chat-turn-break' : ''} {assistantClass(beat.message)}"
    >
      {#if beat.message.role === "assistant"}<p class="response-context">Medousa</p>{/if}
      <LiquidChatMessage
        message={beat.message}
        afterNode={inlineAgents}
        delegationNodeIds={delegationNodes(beat.message)}
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
  {#if beat.kind === "pair" || beat.message.role === "user"}
    {@render agentGroup(beat.kind === "pair" ? beat.user.id : beat.message.id)}
  {/if}
  {#if beat.kind === "pair"}
    <article class="group relative {assistantClass(beat.assistant)}">
      <p class="response-context">Medousa · Reply to your message</p>
      <LiquidChatMessage
        message={beat.assistant}
        {sessionId} {mobile} {compact} {onPromoteToFlow} {onSubmitIntent} {onOpenCardDetail}
        onRetryWorker={retryWorkerSynthesis}
        afterNode={inlineAgents}
        delegationNodeIds={delegationNodes(beat.assistant)}
      />
      {@render turnActions(beat.assistant, beat.user)}
    </article>
  {/if}
  </VirtualChatBeat>
{/each}

<style>
  .response-context { margin: 8px 0; font-size: 12px; color: rgb(var(--theme-text-tertiary)); }
</style>
