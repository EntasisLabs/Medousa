<script lang="ts">
  import type { WorkCard } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
  import { columnAccentBorder } from "$lib/utils/kanban";
  import { isActionableBlockedCard } from "$lib/utils/workCardRetention";
  import { workHubLayer } from "$lib/utils/workHub";

  interface Props {
    /** Visible cards from the active workshop, including waiting and finished work. */
    cards: WorkCard[];
    selectedId: string | null;
    onSelect: (id: string) => void | Promise<void>;
  }

  let { cards, selectedId, onSelect }: Props = $props();

  const groups = $derived.by(() => {
    const decision: WorkCard[] = [];
    const progress: WorkCard[] = [];
    const finished: WorkCard[] = [];
    for (const card of cards) {
      if (isActionableBlockedCard(card)) decision.push(card);
      else if (["settled", "failed", "stopped"].includes(workHubLayer(card))) finished.push(card);
      else progress.push(card);
    }
    // Retention is applied by the workshop source; newest outcomes come first.
    finished.sort((a, b) => (Date.parse(b.updated_at_utc) || 0) - (Date.parse(a.updated_at_utc) || 0));
    return [
      { label: "Needs your decision", cards: decision },
      { label: "In progress", cards: progress },
      { label: "Recently finished", cards: finished },
    ];
  });
</script>

{#if cards.length === 0}
  <p class="workshop-faint px-1 py-2 text-center text-[11px]">No recent activity</p>
{:else}
  <div class="work-motion-groups">
    {#each groups as group (group.label)}
      {#if group.cards.length > 0}
        <section aria-label={group.label}>
          <h3 class="workshop-faint px-1 pb-1 pt-2 text-[11px]">{group.label}</h3>
          <ul class="work-motion-peek-list" aria-label={group.label}>
            {#each group.cards as card (card.id)}
              <li>
                <button
                  type="button"
                  class="work-motion-peek-card {columnAccentBorder(card.column)} {selectedId === card.id ? 'work-motion-peek-card--selected' : ''}"
                  aria-current={selectedId === card.id ? "true" : undefined}
                  onclick={() => onSelect(card.id)}
                >
                  <p class="line-clamp-2 text-left text-sm leading-snug text-surface-100">
                    {formatCardTitle(card)}
                  </p>
                  <p class="mt-1 truncate text-left text-[11px] text-content-quiet">
                    {formatStatusLabel(card.status_label)}
                  </p>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

<style>
  .work-motion-peek-list { margin: 0; padding: 0; list-style: none; display: grid; gap: 4px; }
  section + section { border-top: 1px solid rgb(var(--theme-border)); margin-top: 8px; }
  .work-motion-peek-card { display: block; width: 100%; padding: 9px 6px; border: 0; border-radius: 6px; background: transparent; color: rgb(var(--theme-text-primary)); cursor: pointer; }
  .work-motion-peek-card:hover { background: rgb(var(--theme-card-hover)); }
  .work-motion-peek-card--selected { outline: 1px solid rgb(var(--theme-link)); }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); }
</style>
    {/each}
  </div>
{/if}
