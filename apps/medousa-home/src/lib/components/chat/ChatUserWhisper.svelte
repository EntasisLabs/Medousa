<script lang="ts">
  /**
   * Quiet user prompt beat. Prompts stay expanded unless the user explicitly
   * collapses one; changing their height as they cross the viewport makes long
   * conversations jump during inertial scrolling.
   */
  import { userWhisperHook } from "$lib/utils/chatTurnBeats";
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import ChatForkMenu from "$lib/components/chat/ChatForkMenu.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { ChatMessage } from "$lib/types/chat";
  import { hostContextLabel } from "$lib/types/turnParts";

  interface Props {
    message: ChatMessage;
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    /** Keep open for the latest turn while the assistant is streaming. */
    forceExpand?: boolean;
    onSubmitIntent?: (text: string) => void;
    onFork?: (includeDraft: boolean) => void | Promise<void>;
    forkBusy?: boolean;
    forkHasDraft?: boolean;
  }

  let {
    message,
    sessionId,
    mobile = false,
    compact = false,
    forceExpand = false,
    onSubmitIntent,
    onFork,
    forkBusy = false,
    forkHasDraft = false,
  }: Props = $props();

  let collapsed = $state(false);

  const trimmed = $derived(message.content?.trim() ?? "");
  const hook = $derived(userWhisperHook(trimmed));
  const contextLabel = $derived(hostContextLabel(message.hostContext));
  const expanded = $derived(forceExpand || !collapsed);
  const speakerLabel = $derived.by(() => {
    const speaker = message.speakerProfileId?.trim();
    if (!speaker) return "You";
    if (
      speaker === userProfiles.activeProfileId ||
      speaker === userProfiles.resolvedUserId
    ) {
      return "You";
    }
    const profile = userProfiles.profiles.find(
      (entry) => entry.profile_id === speaker,
    );
    return profile?.display_name?.trim() || speaker.replace(/^user:/, "");
  });

  function toggleCollapsed() {
    if (forceExpand) return;
    collapsed = !collapsed;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleCollapsed();
    }
  }
</script>

{#if trimmed}
  <div
    class="chat-user-whisper"
    class:chat-user-whisper-expanded={expanded}
    class:chat-user-whisper-compact={compact}
    class:chat-user-whisper-mobile={mobile}
    data-chat-user-prompt
    data-chat-user-message-id={message.id}
  >
    <button
      type="button"
      class="chat-user-whisper-summary"
      aria-expanded={expanded}
      onclick={toggleCollapsed}
      onkeydown={onKeydown}
    >
      <span class="chat-user-whisper-label">{speakerLabel}</span>
      {#if !expanded && hook}
        <span class="chat-user-whisper-dot" aria-hidden="true">·</span>
        <span class="chat-user-whisper-hook">{hook}</span>
      {/if}
    </button>

    <div class="chat-user-whisper-body" inert={!expanded}>
      <LiquidChatMessage {message} {sessionId} {mobile} {compact} {onSubmitIntent} />
      {#if contextLabel}
        <div class="chat-user-whisper-context" title="Context supplied with this turn">
          {contextLabel}
        </div>
      {/if}
      {#if onFork}
        <div class="chat-user-whisper-actions">
          <ChatForkMenu
            hasDraft={forkHasDraft}
            busy={forkBusy}
            {mobile}
            {onFork}
          />
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .chat-user-whisper {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.2rem;
    width: 100%;
    min-width: 0;
    margin-bottom: 0.35rem;
    opacity: 0.55;
    transition: opacity 0.28s ease;
  }

  .chat-user-whisper-expanded {
    opacity: 1;
  }

  .chat-user-whisper-summary {
    display: inline-flex;
    max-width: min(100%, 28rem);
    align-items: center;
    gap: 0.35rem;
    margin: 0;
    padding: 0.15rem 0.1rem;
    border: 0;
    background: transparent;
    color: rgb(var(--theme-text-quiet));
    cursor: pointer;
    text-align: right;
    font-size: 0.6875rem;
    line-height: 1.35;
  }

  .chat-user-whisper-summary:hover,
  .chat-user-whisper-summary:focus-visible {
    color: rgb(var(--theme-text-secondary));
  }

  .chat-user-whisper-label {
    font-weight: 600;
    letter-spacing: 0.02em;
    color: inherit;
  }

  .chat-user-whisper-dot {
    opacity: 0.7;
  }

  .chat-user-whisper-hook {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgb(var(--theme-text-tertiary));
  }

  .chat-user-whisper-body {
    max-width: min(100%, 32rem);
    max-height: 0;
    overflow: hidden;
    opacity: 0;
    transform: translateY(-0.15rem);
    transition:
      max-height 0.32s ease,
      opacity 0.28s ease,
      transform 0.28s ease;
    text-align: right;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-200));
  }

  .chat-user-whisper-expanded .chat-user-whisper-body {
    max-height: none;
    opacity: 1;
    transform: translateY(0);
    overflow: visible;
  }

  .chat-user-whisper-compact .chat-user-whisper-body {
    max-width: min(100%, 26rem);
    font-size: 0.75rem;
  }

  .chat-user-whisper-mobile .chat-user-whisper-summary {
    max-width: min(100%, 85%);
  }

  .chat-user-whisper-mobile .chat-user-whisper-body {
    max-width: min(100%, 92%);
  }

  .chat-user-whisper-body :global(.liquid-prose-plain) {
    margin: 0;
    text-align: right;
    color: rgb(var(--color-surface-200));
  }

  .chat-user-whisper-context {
    width: fit-content;
    max-width: 100%;
    margin: 0.35rem 0 0 auto;
    padding: 0.18rem 0.48rem;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.42);
    border-radius: 999px;
    color: rgb(var(--theme-text-tertiary));
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.625rem;
    line-height: 1.35;
  }

  .chat-user-whisper-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.2rem;
  }

  .chat-user-whisper:hover .chat-user-whisper-actions :global(.chat-turn-action),
  .chat-user-whisper:focus-within .chat-user-whisper-actions :global(.chat-turn-action) {
    opacity: 1;
  }

  @media (prefers-reduced-motion: reduce) {
    .chat-user-whisper,
    .chat-user-whisper-body {
      transition: none;
    }
  }
</style>
