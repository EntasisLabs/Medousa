<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import WorkHubTrays from "$lib/components/work/WorkHubTrays.svelte";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { retryWorkspaceCard } from "$lib/daemon";
  import type { ProvenanceChip } from "$lib/utils/workHub";
  import { partitionWorkHub } from "$lib/utils/workHub";

  interface Props {
    visible: boolean;
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenNote?: (path: string) => void;
    onOpenChat?: () => void;
  }

  let {
    visible,
    onSelectCard,
    onOpenNote = () => {},
    onOpenChat = () => {},
  }: Props = $props();

  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);
  const statusLine = $derived.by(() => {
    if (living.length > 0) {
      return living.length === 1 ? "1 in motion · pull to refresh" : `${living.length} in motion · pull to refresh`;
    }
    if (partition.stuck.length > 0) {
      return `${partition.stuck.length} stuck · pull to refresh`;
    }
    return "All clear · pull to refresh";
  });

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

  async function handleProvenance(chip: ProvenanceChip, cardId: string) {
    if (chip.kind === "vault" && chip.href) {
      onOpenNote(chip.href);
      return;
    }
    if (chip.kind === "chat") {
      const detail = workspace.cardDetailsCache.get(cardId);
      const sessionId = detail?.session_id?.trim();
      if (sessionId && sessionId !== chat.sessionId) {
        await chat.switchSession(sessionId);
      }
      onOpenChat();
      return;
    }
    void onSelectCard(cardId);
  }
</script>

<div class="relative flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  <p class="mobile-work-status shrink-0 px-4 py-3 text-center text-xs text-surface-400">
    {refreshing ? "Refreshing…" : statusLine}
  </p>

  <div
    bind:this={scrollEl}
    class="mobile-pull-scroll min-h-0 flex-1 overflow-y-auto"
    role="region"
    aria-label="Work hub"
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

    <div class="px-4 py-3">
      {#if living.length === 0}
        <EmptyState
          title="Nothing in motion"
          description="Tap + to ask Medousa something new."
        />
      {:else}
        <div class="work-hub-grid pb-2">
          {#each living as card (card.id)}
            <WorkManifestCard
              {card}
              detail={workspace.cardDetailsCache.get(card.id)}
              selected={workspace.selectedCardId === card.id}
              onSelect={(id) => void onSelectCard(id)}
              onProvenance={handleProvenance}
            />
          {/each}
        </div>
      {/if}

      <WorkHubTrays onSelectCard={onSelectCard} />
    </div>
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
