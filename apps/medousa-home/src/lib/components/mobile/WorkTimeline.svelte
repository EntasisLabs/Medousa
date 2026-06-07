<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import MobileWorkCard from "$lib/components/mobile/MobileWorkCard.svelte";
  import { haptic } from "$lib/haptics";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { retryWorkspaceCard } from "$lib/daemon";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import { columnAccentBorder } from "$lib/utils/kanban";

  interface Props {
    visible: boolean;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onSelectCard }: Props = $props();

  const needsYou = $derived(
    workspace.cards.filter((card) => card.column === "blocked"),
  );
  const inMotion = $derived(
    workspace.cards.filter((card) =>
      ["backlog", "in_flight", "wrapping_up"].includes(card.column),
    ),
  );
  const doneToday = $derived(
    workspace.cards.filter((card) => card.column === "done"),
  );

  let doneOpen = $state(false);
  let scrollEl: HTMLDivElement | undefined = $state();
  let pullY = $state(0);
  let refreshing = $state(false);
  let touchStartY = 0;
  let pulling = false;

  let toastMessage = $state<string | null>(null);
  let toastCardId = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (visible) {
      void workspace.prefetchCardDetails();
    }
  });

  async function refresh() {
    await workspace.prefetchCardDetails();
  }

  function onTouchStart(event: TouchEvent) {
    if (!scrollEl || scrollEl.scrollTop > 2 || refreshing) return;
    touchStartY = event.touches[0].clientY;
    pulling = true;
  }

  function onTouchMove(event: TouchEvent) {
    if (!pulling || !scrollEl || scrollEl.scrollTop > 2) return;
    const delta = event.touches[0].clientY - touchStartY;
    if (delta > 0) {
      pullY = Math.min(delta * 0.45, 72);
    }
  }

  async function onTouchEnd() {
    if (!pulling) return;
    pulling = false;
    if (pullY >= 48) {
      refreshing = true;
      try {
        await refresh();
        haptic("success");
      } finally {
        refreshing = false;
      }
    }
    pullY = 0;
  }

  function showCancelToast(cardId: string, message: string) {
    if (toastTimer) clearTimeout(toastTimer);
    toastCardId = cardId;
    toastMessage = message || "Canceled";
    toastTimer = setTimeout(dismissToast, 5000);
  }

  function dismissToast() {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = null;
    toastCardId = null;
  }

  async function undoCancel() {
    if (!toastCardId) return;
    const cardId = toastCardId;
    dismissToast();
    haptic("light");
    try {
      await retryWorkspaceCard(cardId);
    } catch (err) {
      toastMessage = err instanceof Error ? err.message : String(err);
      toastCardId = null;
      toastTimer = setTimeout(dismissToast, 4000);
    }
  }
</script>

<div class="relative flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header flex items-center justify-between">
    <h1 class="text-sm font-semibold">Work</h1>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      onclick={refresh}
      disabled={refreshing}
    >
      {refreshing ? "…" : "Refresh"}
    </button>
  </header>

  <div
    bind:this={scrollEl}
    class="mobile-pull-scroll min-h-0 flex-1 overflow-y-auto px-4 py-3"
    role="region"
    aria-label="Work timeline"
    ontouchstart={onTouchStart}
    ontouchmove={onTouchMove}
    ontouchend={onTouchEnd}
  >
    {#if pullY > 0 || refreshing}
      <div
        class="mobile-pull-indicator"
        style:height="{pullY || (refreshing ? 32 : 0)}px"
      >
        <span class="workshop-faint text-xs">
          {refreshing ? "Refreshing…" : pullY >= 48 ? "Release to refresh" : "Pull to refresh"}
        </span>
      </div>
    {/if}

    {#if needsYou.length > 0}
      <section class="mb-5">
        <h2 class="workshop-section-title text-warning-300">Needs you</h2>
        <ul class="mt-2 space-y-2">
          {#each needsYou as card (card.id)}
            <li>
              <button
                type="button"
                class="workshop-kanban-card w-full {columnAccentBorder(card.column)}"
                onclick={() => onSelectCard(card.id)}
              >
                <p class="text-sm font-medium text-surface-100">
                  {formatCardTitle(card)}
                </p>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <section class="mb-5">
      <h2 class="workshop-section-title">In motion</h2>
      {#if inMotion.length === 0}
        <p class="workshop-faint mt-3">Nothing running right now.</p>
      {:else}
        <ul class="mt-2 space-y-2">
          {#each inMotion as card (card.id)}
            <li>
              <MobileWorkCard
                {card}
                swipeCancel={workspace.isCancellable(card)}
                onSelect={() => onSelectCard(card.id)}
                onCanceled={showCancelToast}
              />
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if doneToday.length > 0}
      <section>
        <button
          type="button"
          class="flex w-full items-center justify-between text-left"
          onclick={() => (doneOpen = !doneOpen)}
        >
          <h2 class="workshop-section-title">Done today</h2>
          <span class="workshop-faint">{doneOpen ? "▾" : "▸"} {doneToday.length}</span>
        </button>
        {#if doneOpen}
          <ul class="mt-2 space-y-2">
            {#each doneToday as card (card.id)}
              <li>
                <button
                  type="button"
                  class="workshop-kanban-card w-full opacity-80 {columnAccentBorder(card.column)}"
                  onclick={() => onSelectCard(card.id)}
                >
                  <p class="text-sm text-surface-200">{formatCardTitle(card)}</p>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if needsYou.length === 0 && inMotion.length === 0 && doneToday.length === 0}
      <EmptyState
        title="Nothing in motion"
        description="Tap + to queue a new ask — skills attach as metadata, not prompt stuffing."
      />
    {/if}
  </div>

  <button
    type="button"
    class="mobile-fab"
    aria-label="New ask"
    onclick={() => layout.openAskSheet()}
  >
    <Plus size={24} strokeWidth={2} />
  </button>

  <MobileToast
    message={toastMessage}
    actionLabel="Undo"
    onAction={undoCancel}
    onDismiss={dismissToast}
  />
</div>
