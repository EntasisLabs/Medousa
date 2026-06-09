<script lang="ts">
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { partitionWorkHub } from "$lib/utils/workHub";

  interface Props {
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { onSelectCard }: Props = $props();

  type TrayId = "settled" | "failed" | "stopped" | "stuck";

  let openTray = $state<TrayId | null>(null);

  const partition = $derived(partitionWorkHub(workspace.cards));

  const trays = $derived([
    { id: "settled" as const, label: "Settled", cards: partition.settled },
    { id: "failed" as const, label: "Failed", cards: partition.failed },
    { id: "stopped" as const, label: "Stopped", cards: partition.stopped },
    { id: "stuck" as const, label: "Stuck", cards: partition.stuck },
  ]);

  function toggleTray(id: TrayId) {
    openTray = openTray === id ? null : id;
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
</section>
