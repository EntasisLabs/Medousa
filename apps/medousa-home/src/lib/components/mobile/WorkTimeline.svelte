<script lang="ts">
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import NewWorkAsk from "$lib/components/work/NewWorkAsk.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
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

  $effect(() => {
    if (visible) {
      void workspace.prefetchCardDetails();
    }
  });

  async function refresh() {
    await workspace.prefetchCardDetails();
  }
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header flex items-center justify-between">
    <h1 class="text-sm font-semibold">Work</h1>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      onclick={refresh}
    >
      Refresh
    </button>
  </header>

  <div class="mobile-pull-scroll min-h-0 flex-1 overflow-y-auto px-4 py-3">
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
                <p class="workshop-faint mt-0.5 capitalize">
                  {formatStatusLabel(card.status_label)}
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
              <button
                type="button"
                class="workshop-kanban-card w-full {columnAccentBorder(card.column)} {card.column ===
                'wrapping_up'
                  ? 'animate-pulse'
                  : ''}"
                onclick={() => onSelectCard(card.id)}
              >
                <p class="text-sm font-medium text-surface-100">
                  {formatCardTitle(card)}
                </p>
                <p class="workshop-faint mt-0.5 capitalize">
                  {formatStatusLabel(card.status_label)}
                </p>
              </button>
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
        description="Queue a new ask below — skills attach as metadata, not prompt stuffing."
      />
    {/if}
  </div>

  <NewWorkAsk {visible} />
</div>
