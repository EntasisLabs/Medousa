<script lang="ts">
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { findBlockedGroupForCard, prepareBlockedColumn } from "$lib/utils/groupWork";
  import { partitionWorkHub, type WorkHubLayer } from "$lib/utils/workHub";

  interface Props {
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { onSelectCard }: Props = $props();

  type TrayId = WorkHubLayer;

  let openTray = $state<TrayId | null>(null);
  let trayBusy = $state(false);

  const partition = $derived(partitionWorkHub(workspace.visibleCards()));
  const blockedDisplay = $derived(prepareBlockedColumn(workspace.cards));

  const trays = $derived([
    { id: "settled" as const, label: "Settled", cards: partition.settled },
    { id: "failed" as const, label: "Failed", cards: partition.failed },
    { id: "stopped" as const, label: "Stopped", cards: partition.stopped },
    { id: "stuck" as const, label: "Stuck", cards: partition.stuck },
  ]);

  function toggleTray(id: TrayId) {
    openTray = openTray === id ? null : id;
  }

  async function archiveSettled() {
    if (trayBusy || partition.settled.length === 0) return;
    trayBusy = true;
    try {
      await workspace.archiveTrayCards(partition.settled);
    } finally {
      trayBusy = false;
    }
  }

  async function clearFailed() {
    if (trayBusy || partition.failed.length === 0) return;
    trayBusy = true;
    try {
      await workspace.archiveTerminalTrayCards(partition.failed, "failed");
    } finally {
      trayBusy = false;
    }
  }

  async function clearStopped() {
    if (trayBusy || partition.stopped.length === 0) return;
    trayBusy = true;
    try {
      await workspace.archiveTerminalTrayCards(partition.stopped, "stopped");
    } finally {
      trayBusy = false;
    }
  }

  async function hideStuckGroup() {
    if (trayBusy || partition.stuck.length === 0) return;
    const lead = partition.stuck[0];
    const group = findBlockedGroupForCard(workspace.cards, lead.id);
    if (!group) return;
    trayBusy = true;
    try {
      await workspace.dismissBlockedGroup(group);
    } finally {
      trayBusy = false;
    }
  }

  async function retryStuckGroup() {
    if (trayBusy || partition.stuck.length === 0) return;
    const lead = partition.stuck[0];
    const group = findBlockedGroupForCard(workspace.cards, lead.id);
    if (!group) return;
    trayBusy = true;
    try {
      await workspace.retryBlockedGroup(group);
    } finally {
      trayBusy = false;
    }
  }
</script>

<section class="work-hub-trays" aria-label="Settled work trays">
  <div class="work-hub-tray-tabs">
    {#each trays as tray (tray.id)}
      {#if tray.cards.length > 0}
        <button
          type="button"
          class="work-hub-tray-tab {openTray === tray.id ? 'work-hub-tray-tab-open' : ''}"
          onclick={() => toggleTray(tray.id)}
        >
          {tray.label}
          <span class="tabular-nums text-surface-500">({tray.cards.length})</span>
        </button>
      {/if}
    {/each}
  </div>

  {#if openTray}
    {@const active = trays.find((tray) => tray.id === openTray)}
    {#if active && active.cards.length > 0}
      <div class="work-hub-tray-panel">
        <div class="mb-2 flex flex-wrap items-center gap-2">
          {#if active.id === "settled"}
            <button
              type="button"
              class="workshop-text-action"
              disabled={trayBusy}
              onclick={() => void archiveSettled()}
            >
              Archive settled
            </button>
          {:else if active.id === "failed"}
            <button
              type="button"
              class="workshop-text-action"
              disabled={trayBusy}
              onclick={() => void clearFailed()}
            >
              Clear failed
            </button>
          {:else if active.id === "stopped"}
            <button
              type="button"
              class="workshop-text-action"
              disabled={trayBusy}
              onclick={() => void clearStopped()}
            >
              Clear stopped
            </button>
          {:else if active.id === "stuck"}
            <button
              type="button"
              class="workshop-text-action"
              disabled={trayBusy}
              onclick={() => void retryStuckGroup()}
            >
              Retry all
            </button>
            <button
              type="button"
              class="workshop-text-action"
              disabled={trayBusy}
              onclick={() => void hideStuckGroup()}
            >
              Hide stuck
            </button>
            {#if blockedDisplay.overflow > 0}
              <span class="text-[10px] text-surface-500">
                +{blockedDisplay.overflow} more grouped
              </span>
            {/if}
          {/if}
        </div>
        <div class="work-hub-tray-grid">
          {#each active.cards.slice(0, 12) as card (card.id)}
            <WorkManifestCard
              {card}
              detail={workspace.cardDetailsCache.get(card.id)}
              compact={true}
              selected={workspace.selectedCardId === card.id}
              onSelect={(id) => void onSelectCard(id)}
            />
          {/each}
        </div>
        {#if active.cards.length > 12}
          <p class="work-hub-tray-more">
            +{active.cards.length - 12} more in {active.label.toLowerCase()}
          </p>
        {/if}
      </div>
    {/if}
  {/if}

  {#if workspace.cardActionMessage}
    <p class="mt-1 text-[10px] text-surface-500">{workspace.cardActionMessage}</p>
  {/if}
</section>
