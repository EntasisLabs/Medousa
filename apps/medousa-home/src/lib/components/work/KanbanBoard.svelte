<script lang="ts">
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import CancelDropZone from "$lib/components/work/CancelDropZone.svelte";
  import KanbanCard from "$lib/components/work/KanbanCard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { SwimlaneMode } from "$lib/types/work";
  import { columnLabel } from "$lib/types/workspace";
  import { buildKanbanColumns, columnTone } from "$lib/utils/kanban";

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
  <header
    class="flex flex-wrap items-center justify-between gap-3 border-b border-surface-500/20 px-4 py-3"
  >
    <div>
      <h1 class="text-base font-semibold">Work board</h1>
      <p class="text-xs text-surface-400">Cards update live as work moves</p>
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

  <div class="relative flex min-h-0 flex-1 gap-3 overflow-x-auto px-4 pb-4">
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
        class="flex w-64 shrink-0 flex-col rounded-container-token border {columnTone(
          column.column,
        )}"
      >
        <header class="border-b border-surface-500/20 px-3 py-2">
          <h2 class="text-sm font-semibold capitalize">
            {columnLabel(column.column)}
          </h2>
          <p class="text-xs text-surface-400">{column.cards.length}</p>
        </header>

        <div class="flex-1 space-y-2 overflow-y-auto p-2">
          {#if workspace.swimlane === "none"}
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
