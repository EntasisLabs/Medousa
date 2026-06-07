<script lang="ts">
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import CancelDropZone from "$lib/components/work/CancelDropZone.svelte";
  import KanbanCard from "$lib/components/work/KanbanCard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { SwimlaneMode } from "$lib/types/work";
  import { columnLabel } from "$lib/types/workspace";
  import { buildKanbanColumns, columnAccent, columnTone } from "$lib/utils/kanban";
  import { prepareBlockedColumn } from "$lib/utils/groupWork";

  interface Props {
    onSelectCard: (id: string) => void;
  }

  let { onSelectCard }: Props = $props();

  const swimlaneOptions: { id: SwimlaneMode; label: string }[] = [
    { id: "none", label: "No swimlanes" },
    { id: "intent", label: "By intent" },
    { id: "manuscript", label: "By manuscript" },
    { id: "job_family", label: "By job family" },
    { id: "session", label: "By session" },
  ];

  const columns = $derived(
    buildKanbanColumns(
      workspace.cards,
      workspace.cardDetailsCache,
      workspace.swimlane,
      workspace.showDone,
    ),
  );

  const blockedDisplay = $derived(prepareBlockedColumn(workspace.cards));

  const hasCards = $derived(
    columns.some((column) =>
      workspace.swimlane === "none"
        ? column.cards.length > 0
        : column.lanes.some((lane) => lane.cards.length > 0),
    ),
  );

  function handleSwimlaneChange(event: Event) {
    workspace.swimlane = (event.currentTarget as HTMLSelectElement)
      .value as SwimlaneMode;
    void workspace.prefetchCardDetails();
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col">
  <header class="workshop-header flex flex-wrap items-center justify-between gap-3">
    <div>
      <h1 class="text-sm font-semibold">Work</h1>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <label class="flex items-center gap-2 text-xs text-surface-300">
        <span>Swimlanes</span>
        <select
          class="select text-xs"
          value={workspace.swimlane}
          onchange={handleSwimlaneChange}
        >
          {#each swimlaneOptions as option (option.id)}
            <option value={option.id}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="flex items-center gap-2 text-xs text-surface-300">
        <input
          type="checkbox"
          class="checkbox"
          checked={workspace.showDone}
          onchange={(event) => {
            workspace.showDone = (event.currentTarget as HTMLInputElement).checked;
          }}
        />
        Show done
      </label>
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={() => workspace.prefetchCardDetails()}
      >
        Refresh lanes
      </button>
    </div>
  </header>

  <CancelDropZone onCanceled={() => {}} />

  <div class="relative flex min-h-0 flex-1 gap-2 overflow-x-auto px-3 pb-3">
    {#if !hasCards}
      <div class="absolute inset-0 flex items-center justify-center">
        <EmptyState
          title="Nothing in motion"
          description="When work starts, cards land here — backlog, in flight, and wrapping up."
        />
      </div>
    {/if}
    {#each columns as column (column.column)}
      <div
        class="flex w-52 shrink-0 flex-col rounded-md border {columnTone(
          column.column,
        )}"
      >
        <header class="border-b border-surface-500/40 bg-surface-800/35 px-2.5 py-2">
          <div class="flex items-center gap-2">
            <span
              class="h-2 w-2 shrink-0 rounded-full {columnAccent(column.column)}"
              aria-hidden="true"
            ></span>
            <h2 class="text-xs font-medium capitalize text-surface-200">
              {columnLabel(column.column)}
            </h2>
            <span class="ml-auto text-xs tabular-nums text-surface-500">
              {column.column === "blocked"
                ? blockedDisplay.total
                : column.cards.length}
            </span>
          </div>
        </header>

        <div class="flex-1 space-y-1 overflow-y-auto p-1.5">
          {#if column.column === "blocked" && workspace.swimlane === "none"}
            {#each blockedDisplay.items as item (item.card.id)}
              <KanbanCard
                card={item.card}
                groupCount={item.count}
                selected={workspace.selectedCardId === item.card.id}
                onSelect={onSelectCard}
              />
            {:else}
              {#if blockedDisplay.total === 0}
                <p class="px-2 py-6 text-center text-xs text-surface-500">Empty</p>
              {/if}
            {/each}
            {#if blockedDisplay.overflow > 0}
              <p class="px-2 py-3 text-center text-xs text-surface-400">
                +{blockedDisplay.overflow} more blocked
              </p>
            {/if}
          {:else if workspace.swimlane === "none"}
            {#each column.cards as card (card.id)}
              <KanbanCard
                {card}
                selected={workspace.selectedCardId === card.id}
                onSelect={onSelectCard}
              />
            {:else}
              <p class="px-2 py-6 text-center text-xs text-surface-500">Empty</p>
            {/each}
          {:else}
            {#each column.lanes as lane (lane.key)}
              <div class="space-y-2">
                <p class="px-1 text-xs font-medium uppercase tracking-wide text-surface-400">
                  {lane.label}
                </p>
                {#each lane.cards as card (card.id)}
                  <KanbanCard
                    {card}
                    selected={workspace.selectedCardId === card.id}
                    onSelect={onSelectCard}
                  />
                {/each}
              </div>
            {:else}
              <p class="px-2 py-6 text-center text-xs text-surface-500">Empty</p>
            {/each}
          {/if}
        </div>
      </div>
    {/each}
  </div>
</section>
