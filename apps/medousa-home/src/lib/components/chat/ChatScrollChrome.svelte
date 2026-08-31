<script lang="ts">
  import { ArrowDown, LoaderCircle } from "@lucide/svelte";
  import MarkdownHeadingOutline from "$lib/components/ui/MarkdownHeadingOutline.svelte";
  import { haptic } from "$lib/haptics";
  import { tick, type Snippet } from "svelte";

  interface TurnItem {
    id: string;
    text: string;
    depth: number;
  }

  interface Props {
    mobile: boolean;
    pinThresholdPx: number;
    showFab: boolean;
    showTurnRail: boolean;
    showCurrentTurnAnchor: boolean;
    latestUserPreview: string;
    latestUserTurnId: string | null;
    chatTurnItems: TurnItem[];
    activeChatTurnId: string | null;
    chatScrolling: boolean;
    bodyClass: string;
    scrollClass: string;
    historyKey?: string;
    canLoadOlder?: boolean;
    loadingOlder?: boolean;
    onLoadOlder?: () => Promise<unknown>;
    children?: Snippet;
    onAtBottomChange: (atBottom: boolean) => void;
    scrollEl?: HTMLDivElement;
    scrollToLatest?: (force?: boolean, behavior?: ScrollBehavior) => void;
    scheduleChatNavigationMeasure?: () => void;
    resetForSession?: () => void;
  }

  let {
    mobile,
    pinThresholdPx,
    showFab,
    showTurnRail,
    showCurrentTurnAnchor,
    latestUserPreview,
    latestUserTurnId,
    chatTurnItems,
    activeChatTurnId = $bindable(null),
    chatScrolling = $bindable(false),
    bodyClass,
    scrollClass,
    historyKey = "",
    canLoadOlder = false,
    loadingOlder = false,
    onLoadOlder,
    children,
    onAtBottomChange,
    scrollEl = $bindable(),
    scrollToLatest = $bindable((_force?: boolean, _behavior?: ScrollBehavior) => {}),
    scheduleChatNavigationMeasure = $bindable(() => {}),
    resetForSession = $bindable(() => {}),
  }: Props = $props();

  let atBottom = $state(true);
  let pinLatestUserTurn = $state(false);
  let chatNavigationFrame = 0;
  let chatScrollEndTimer: ReturnType<typeof setTimeout> | undefined;
  let historySentinel = $state<HTMLDivElement>();
  let historyLoadInFlight = $state(false);
  let historyLoadFailed = $state(false);
  let historyNavigationReady = $state(false);
  let observedHistoryKey = "";

  function shouldLoadOlderAtTop(): boolean {
    return Boolean(historyNavigationReady && scrollEl && scrollEl.scrollTop <= 160);
  }

  async function requestOlder(force = false) {
    if (
      !scrollEl ||
      !onLoadOlder ||
      !canLoadOlder ||
      loadingOlder ||
      historyLoadInFlight ||
      (!force && !shouldLoadOlderAtTop()) ||
      (historyLoadFailed && !force)
    ) {
      return;
    }

    const previousHeight = scrollEl.scrollHeight;
    const previousTop = scrollEl.scrollTop;
    historyLoadInFlight = true;
    historyLoadFailed = false;
    try {
      await onLoadOlder();
      historyLoadInFlight = false;
      await tick();
      if (!scrollEl) return;
      const addedHeight = scrollEl.scrollHeight - previousHeight;
      scrollEl.scrollTop = Math.max(0, previousTop + addedHeight);
      scheduleChatNavigationMeasureFn();
    } catch {
      historyLoadFailed = true;
    } finally {
      historyLoadInFlight = false;
      await tick();
      if (!historyLoadFailed && shouldLoadOlderAtTop() && canLoadOlder) {
        queueMicrotask(() => void requestOlder());
      }
    }
  }

  function scrollToLatestFn(force = false, behavior: ScrollBehavior = "auto") {
    if (!scrollEl) return;
    if (!force && !atBottom) return;
    requestAnimationFrame(() => {
      if (!scrollEl) return;
      if (!force && !atBottom) return;
      scrollEl.scrollTo({ top: scrollEl.scrollHeight, behavior });
      atBottom = true;
      onAtBottomChange(true);
      historyNavigationReady = true;
      if (shouldLoadOlderAtTop()) void requestOlder();
    });
  }

  function measureChatNavigation() {
    chatNavigationFrame = 0;
    if (!scrollEl) {
      activeChatTurnId = null;
      pinLatestUserTurn = false;
      return;
    }
    const rootRect = scrollEl.getBoundingClientRect();
    const turns = [...scrollEl.querySelectorAll<HTMLElement>("[data-chat-turn-user-id]")];
    if (turns.length === 0) {
      activeChatTurnId = null;
      pinLatestUserTurn = false;
      return;
    }
    const threshold = rootRect.top + 64;
    let activeId = turns[0]?.dataset.chatTurnUserId ?? null;
    for (const turn of turns) {
      if (turn.getBoundingClientRect().top <= threshold) {
        activeId = turn.dataset.chatTurnUserId ?? activeId;
      } else {
        break;
      }
    }
    activeChatTurnId = activeId;
    const latestId = latestUserTurnId;
    const latestTurn = latestId
      ? turns.find((turn) => turn.dataset.chatTurnUserId === latestId)
      : undefined;
    if (!latestTurn) {
      pinLatestUserTurn = false;
      return;
    }
    const latestRect = latestTurn.getBoundingClientRect();
    const responseIsLong = latestRect.height >= Math.max(280, scrollEl.clientHeight * 0.8);
    const promptHasLeftTop = latestRect.top < rootRect.top + 8;
    const responseStillVisible = latestRect.bottom > rootRect.top + 96;
    pinLatestUserTurn = responseIsLong && promptHasLeftTop && responseStillVisible;
  }

  function scheduleChatNavigationMeasureFn() {
    if (chatNavigationFrame) return;
    chatNavigationFrame = requestAnimationFrame(measureChatNavigation);
  }

  function onScroll() {
    if (!scrollEl) return;
    const distanceFromBottom =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    atBottom = distanceFromBottom <= pinThresholdPx;
    onAtBottomChange(atBottom);
    chatScrolling = true;
    if (chatScrollEndTimer) clearTimeout(chatScrollEndTimer);
    chatScrollEndTimer = setTimeout(() => {
      chatScrolling = false;
      chatScrollEndTimer = undefined;
    }, 160);
    scheduleChatNavigationMeasureFn();
    if (shouldLoadOlderAtTop()) void requestOlder();
  }

  function scrollToChatTurn(id: string) {
    if (!scrollEl) return;
    const target = [...scrollEl.querySelectorAll<HTMLElement>("[data-chat-turn-user-id]")].find(
      (element) => element.dataset.chatTurnUserId === id,
    );
    if (!target) return;
    const rootRect = scrollEl.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    scrollEl.scrollTo({
      top: Math.max(0, scrollEl.scrollTop + targetRect.top - rootRect.top - 12),
      behavior: "smooth",
    });
  }

  function scrollToBottomFromFab() {
    if (mobile) haptic("light");
    scrollToLatestFn(true, "smooth");
  }

  function scrollToCurrentTurn() {
    if (latestUserTurnId) scrollToChatTurn(latestUserTurnId);
  }

  function resetForSessionFn() {
    atBottom = true;
    historyNavigationReady = false;
    onAtBottomChange(true);
    activeChatTurnId = null;
    pinLatestUserTurn = false;
  }

  $effect(() => {
    scrollToLatest = scrollToLatestFn;
    scheduleChatNavigationMeasure = scheduleChatNavigationMeasureFn;
    resetForSession = resetForSessionFn;
  });

  $effect(() => {
    const key = historyKey;
    if (key === observedHistoryKey) return;
    observedHistoryKey = key;
    historyLoadFailed = false;
    historyLoadInFlight = false;
  });

  $effect(() => {
    const root = scrollEl;
    const sentinel = historySentinel;
    void historyKey;
    void canLoadOlder;
    void loadingOlder;
    void historyNavigationReady;
    if (!root || !sentinel || !onLoadOlder || typeof IntersectionObserver === "undefined") {
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) void requestOlder();
      },
      { root, rootMargin: "160px 0px 0px", threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  });
</script>

<div class="chat-panel-main">
  {#if showCurrentTurnAnchor && pinLatestUserTurn}
    <button
      type="button"
      class="chat-current-turn-anchor"
      aria-label="Show your latest message"
      onclick={scrollToCurrentTurn}
    >
      <span class="chat-current-turn-anchor-label">You</span>
      <span class="chat-current-turn-anchor-preview">{latestUserPreview}</span>
    </button>
  {/if}
  <div class={bodyClass}>
    <div bind:this={scrollEl} onscroll={onScroll} class={scrollClass}>
      <div
        bind:this={historySentinel}
        class:chat-history-sentinel-active={canLoadOlder || loadingOlder || historyLoadInFlight}
        class="chat-history-sentinel"
        aria-live="polite"
      >
        {#if loadingOlder || historyLoadInFlight}
          <LoaderCircle size={16} class="animate-spin" aria-label="Loading earlier messages" />
        {:else if historyLoadFailed && canLoadOlder}
          <button type="button" class="chat-history-retry" onclick={() => void requestOlder(true)}>
            Load earlier messages
          </button>
        {/if}
      </div>
      {@render children?.()}
    </div>
    {#if showTurnRail}
      <MarkdownHeadingOutline
        items={chatTurnItems}
        activeId={activeChatTurnId}
        scrolling={chatScrolling}
        mode="rail"
        label="Conversation turns"
        onSelect={scrollToChatTurn}
      />
    {/if}
  </div>
</div>

{#if showFab}
  <button
    type="button"
    class="chat-scroll-fab"
    aria-label="Scroll to latest"
    onclick={scrollToBottomFromFab}
  >
    <ArrowDown size={22} strokeWidth={2} />
  </button>
{/if}

<style>
  .chat-panel-main {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .chat-history-sentinel {
    display: flex;
    min-height: 0;
    align-items: center;
    justify-content: center;
    color: rgb(var(--theme-text-tertiary));
  }

  .chat-history-sentinel-active {
    min-height: 1.75rem;
  }

  .chat-history-retry {
    border: 0;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
    cursor: pointer;
  }

  .chat-history-retry:hover,
  .chat-history-retry:focus-visible {
    color: rgb(var(--theme-text-primary));
    outline: none;
  }

  .chat-current-turn-anchor {
    position: absolute;
    top: 0.5rem;
    right: 2.4rem;
    z-index: 4;
    display: flex;
    min-width: 0;
    width: min(32rem, calc(100% - 4.8rem));
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.7rem;
    border: 1px solid rgb(var(--color-surface-400) / 0.18);
    border-radius: 0.75rem;
    background: rgb(var(--color-surface-900) / 0.94);
    color: rgb(var(--color-surface-200));
    text-align: left;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.3);
    backdrop-filter: blur(12px);
    cursor: pointer;
  }

  .chat-current-turn-anchor:hover,
  .chat-current-turn-anchor:focus-visible {
    border-color: rgb(var(--color-surface-300) / 0.32);
    background: rgb(var(--color-surface-800) / 0.95);
    outline: none;
  }

  .chat-current-turn-anchor-label {
    flex-shrink: 0;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.68rem;
    font-weight: 650;
    white-space: nowrap;
  }

  .chat-current-turn-anchor-preview {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    font-size: 0.75rem;
    line-height: 1.35;
  }

  @media (max-width: 640px) {
    .chat-current-turn-anchor {
      right: 0.75rem;
      width: calc(100% - 1.5rem);
    }
  }
</style>
