<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { partitionWorkHub, type WorkHubLayer } from "$lib/utils/workHub";
  import { findBlockedGroupForCard } from "$lib/utils/groupWork";
  import type { WorkCard } from "$lib/types/workspace";
  import type { ActivityFilter } from "$lib/spotlight/activity";

  let { filter, cards }: { filter: ActivityFilter; cards: WorkCard[] } = $props();
  let busy = $state(false);
  let message = $state("");
  const partition = $derived(partitionWorkHub(cards));
  type TaskLayer = Exclude<WorkHubLayer, "living">;
  const layers = $derived<TaskLayer[]>(filter === "attention" ? ["failed", "stuck"] : filter === "finished" ? ["settled", "failed", "stopped"] : []);
  const available = $derived(layers.filter((layer) => partition[layer].length > 0));
  $effect(() => { void filter; message = ""; });

  async function apply(layer: TaskLayer, retry = false) {
    if (busy || connection.offline) return;
    const scope = activeWorkshopId();
    const snapshot = partition[layer].slice(0, 24);
    busy = true; message = "";
    try {
      if (layer === "settled") await workspace.archiveTrayCards(snapshot);
      else if (layer === "failed" || layer === "stopped") await workspace.archiveTerminalTrayCards(snapshot, layer);
      else {
        // Build groups from matching cards so a search never retries hidden tasks.
        const handled = new Set<string>();
        for (const card of snapshot) {
          if (activeWorkshopId() !== scope) break;
          if (handled.has(card.id)) continue;
          const group = findBlockedGroupForCard(snapshot, card.id);
          if (!group) continue;
          group.cards.forEach((entry) => handled.add(entry.id));
          if (retry) await workspace.retryBlockedGroup(group);
          else await workspace.dismissBlockedGroup(group);
        }
      }
      if (activeWorkshopId() === scope) message = workspace.cardActionMessage ?? "";
    } catch (cause) {
      if (activeWorkshopId() === scope) message = cause instanceof Error ? cause.message : String(cause);
    } finally { if (activeWorkshopId() === scope) busy = false; }
  }
</script>

{#if available.length || message}
  <div class="task-activity-actions" aria-label="Task actions">
    {#each available as layer}
      {@const count = Math.min(partition[layer].length, 24)}
      {#if layer === "stuck"}<button disabled={busy || connection.offline} onclick={() => void apply(layer, true)}>Retry stuck ({count})</button>{/if}
      <button disabled={busy || connection.offline} onclick={() => void apply(layer)}>{layer === 'settled' ? 'Archive settled' : layer === 'stuck' ? 'Hide stuck' : layer === 'failed' ? 'Clear failed' : 'Clear stopped'} ({count})</button>
    {/each}
    {#if message}<p role="status">{message}</p>{/if}
  </div>
{/if}

<style>
  .task-activity-actions { display:flex; align-items:center; flex-wrap:wrap; gap:10px; padding:7px 12px; border-bottom:1px solid rgb(var(--theme-border) / .2); color:rgb(var(--theme-text-tertiary)); font-size:11px; }
  button { color:rgb(var(--theme-text)); }
  button:disabled { opacity:.5; }
  p { width:100%; }
</style>
