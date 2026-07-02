<script lang="ts">
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import ChatArtifactStrip from "$lib/components/chat/ChatArtifactStrip.svelte";
  import AssistantThinking from "$lib/components/chat/AssistantThinking.svelte";
  import ChatMediaParts from "$lib/components/chat/ChatMediaParts.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import type { ChatMessage } from "$lib/types/chat";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import { formatToolName } from "$lib/utils/formatTurn";

  interface Props {
    messages: ChatMessage[];
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    onPromoteToFlow?: (
      ref: import("$lib/types/toolHistory").ToolHistorySliceRef,
    ) => void | Promise<void>;
  }

  let { messages, sessionId, mobile = false, compact = false, onPromoteToFlow }: Props = $props();

  function displayStatusLine(message: ChatMessage): string | null {
    return visibleChatStatusLine(message.statusLine, settings.showEngineDetailsInChat);
  }
</script>

{#each messages as message, index (message.id)}
  {@const previous = index > 0 ? messages[index - 1] : null}
  {@const turnBreak = message.role === "user" && previous?.role === "assistant"}
  {#if mobile && message.role === "user"}
    <div class="{turnBreak ? 'chat-turn-break' : ''} mobile-chat-user-row">
      <article class="mobile-chat-bubble-user">
        {#if message.content?.trim()}
          <p class="mobile-chat-user-text">{message.content}</p>
        {/if}
        {#if message.mediaAttachments?.length}
          <ChatMediaParts {sessionId} attachments={message.mediaAttachments} compact />
        {/if}
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
    {#if message.role === "assistant"}
      {#if message.stageWhisper?.trim()}
        <p class="duet-stage-whisper mb-2">{message.stageWhisper}</p>
      {/if}

      {#if message.reasoning?.trim()}
        <AssistantThinking
          reasoning={message.reasoning}
          streaming={Boolean(message.streaming)}
          compact={mobile}
        />
      {:else if message.streaming && !message.content?.trim() && !message.stageWhisper?.trim()}
        <p class="mb-1 flex items-center gap-2 text-[11px] text-surface-500">
          <span
            class="inline-block h-1 w-1 animate-pulse rounded-full bg-primary-400/80"
            aria-hidden="true"
          ></span>
          Thinking…
        </p>
      {/if}

      {#if displayStatusLine(message) && message.streaming}
        <p
          class="mb-2 flex items-center gap-1.5 text-[11px] {message.phase === 'worker_ack' ||
          message.phase === 'awaiting_operator'
            ? 'text-warning-300/90'
            : 'text-primary-200/80'}"
        >
          {#if message.streaming}
            <span
              class="inline-block h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-primary-400"
              aria-hidden="true"
            ></span>
          {/if}
          {displayStatusLine(message)}
        </p>
      {/if}

      {#if message.failed && message.errorLine}
        <p class="chat-turn-error" role="alert">{message.errorLine}</p>
      {/if}
      {#if message.content?.trim()}
        <div class="chat-voice">
          <MarkdownContent content={message.content} openLinksInWeb={true} />
        </div>
      {:else if message.streaming && !message.toolRuns?.length}
        <div class="chat-voice">
          <MarkdownContent content="…" />
        </div>
      {/if}

      {#if message.toolRuns && message.toolRuns.length > 0}
        <div class="mt-4">
          <ToolRunChips
            runs={message.toolRuns}
            {sessionId}
            turnIndex={message.turnIndex}
            {onPromoteToFlow}
            compact={mobile}
            inspectorCollapsed={!message.streaming}
          />
        </div>
      {:else if message.tools && message.tools.length > 0}
        <p class="mt-3 font-mono text-[10px] text-surface-500">
          {message.tools.map((tool) => formatToolName(tool)).join(" · ")}
        </p>
      {/if}

      {#if message.uiArtifacts && message.uiArtifacts.length > 0}
        <ChatArtifactStrip
          {sessionId}
          artifacts={message.uiArtifacts}
          compact={mobile || compact}
        />
      {/if}
    {:else if message.role === "user"}
      {#if message.content?.trim()}
        <p class="whitespace-pre-wrap text-sm leading-relaxed text-surface-100">
          {message.content}
        </p>
      {/if}
      {#if message.mediaAttachments?.length}
        <ChatMediaParts {sessionId} attachments={message.mediaAttachments} {compact} />
      {/if}
    {:else}
      <p class="whitespace-pre-wrap text-sm leading-relaxed text-surface-300">
        {message.content}
      </p>
    {/if}
  </article>
  {/if}
{/each}

<style>
  .chat-turn-error {
    margin: 0 0 0.5rem;
    padding: 0.5rem 0.65rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-error-500) 45%, transparent);
    background: color-mix(in srgb, var(--color-error-500) 10%, transparent);
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
