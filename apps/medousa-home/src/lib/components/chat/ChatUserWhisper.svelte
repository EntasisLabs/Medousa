<script lang="ts">
  /**
   * Quiet user prompt beat — collapses to `You · …` like Thinking,
   * expands near the scroll viewport (or on click / forceExpand).
   */
  import { onDestroy, onMount } from "svelte";
  import { userWhisperHook } from "$lib/utils/chatTurnBeats";
  import LiquidChatMessage from "$lib/components/chat/LiquidChatMessage.svelte";
  import type { ChatMessage } from "$lib/types/chat";

  interface Props {
    message: ChatMessage;
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    /** Scroll container for IntersectionObserver (chat-scroll / mobile-chat-scroll). */
    scrollRoot?: HTMLElement | null;
    /** Keep open for the latest turn while the assistant is streaming. */
    forceExpand?: boolean;
    onSubmitIntent?: (text: string) => void;
  }

  let {
    message,
    sessionId,
    mobile = false,
    compact = false,
    scrollRoot = null,
    forceExpand = false,
    onSubmitIntent,
  }: Props = $props();

  let rootEl: HTMLElement | undefined = $state();
  let nearViewport = $state(true);
  let stickyOpen = $state(false);

  const trimmed = $derived(message.content?.trim() ?? "");
  const hook = $derived(userWhisperHook(trimmed));
  const expanded = $derived(forceExpand || stickyOpen || nearViewport);

  let observer: IntersectionObserver | null = null;

  function disconnectObserver() {
    observer?.disconnect();
    observer = null;
  }

  function connectObserver() {
    disconnectObserver();
    if (typeof IntersectionObserver === "undefined") return;
    if (!rootEl) return;
    observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (!entry) return;
        nearViewport = entry.isIntersecting;
      },
      {
        root: scrollRoot ?? null,
        rootMargin: "18% 0px 18% 0px",
        threshold: [0, 0.15, 0.4],
      },
    );
    observer.observe(rootEl);
  }

  onMount(() => {
    connectObserver();
  });

  $effect(() => {
    scrollRoot;
    rootEl;
    connectObserver();
  });

  onDestroy(() => {
    disconnectObserver();
  });

  function toggleSticky() {
    stickyOpen = !stickyOpen;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleSticky();
    }
  }
</script>

{#if trimmed}
  <div
    bind:this={rootEl}
    class="chat-user-whisper"
    class:chat-user-whisper-expanded={expanded}
    class:chat-user-whisper-compact={compact}
    class:chat-user-whisper-mobile={mobile}
    data-chat-user-prompt
  >
    <button
      type="button"
      class="chat-user-whisper-summary"
      aria-expanded={expanded}
      onclick={toggleSticky}
      onkeydown={onKeydown}
    >
      <span class="chat-user-whisper-label">You</span>
      {#if !expanded && hook}
        <span class="chat-user-whisper-dot" aria-hidden="true">·</span>
        <span class="chat-user-whisper-hook">{hook}</span>
      {/if}
    </button>

    <div class="chat-user-whisper-body" inert={!expanded}>
      <LiquidChatMessage {message} {sessionId} {mobile} {compact} {onSubmitIntent} />
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
    color: rgb(var(--color-surface-500));
    cursor: pointer;
    text-align: right;
    font-size: 0.6875rem;
    line-height: 1.35;
  }

  .chat-user-whisper-summary:hover,
  .chat-user-whisper-summary:focus-visible {
    color: rgb(var(--color-surface-300));
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
    color: rgb(var(--color-surface-400));
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
    max-height: 24rem;
    opacity: 1;
    transform: translateY(0);
    overflow-y: auto;
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

  @media (prefers-reduced-motion: reduce) {
    .chat-user-whisper,
    .chat-user-whisper-body {
      transition: none;
    }
  }
</style>
