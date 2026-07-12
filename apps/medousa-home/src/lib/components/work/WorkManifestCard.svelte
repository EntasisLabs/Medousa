<script lang="ts">
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkCard } from "$lib/types/workspace";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import {
    buildProvenanceChips,
    formatManifestStatusChip,
    type ProvenanceChip,
  } from "$lib/utils/workHub";

  interface Props {
    card: WorkCard;
    detail?: WorkCardDetail | null;
    selected?: boolean;
    compact?: boolean;
    onSelect: (id: string) => void;
    onProvenance?: (chip: ProvenanceChip, cardId: string) => void;
  }

  let {
    card,
    detail = null,
    selected = false,
    compact = false,
    onSelect,
    onProvenance,
  }: Props = $props();

  const chip = $derived(formatManifestStatusChip(card));
  const provenance = $derived(buildProvenanceChips(card, detail));
  const wrappingUp = $derived(card.column === "wrapping_up");

  function chipClass(tone: string): string {
    switch (tone) {
      case "primary":
        return "work-hub-chip-primary";
      case "warning":
        return "work-hub-chip-warning";
      case "danger":
        return "work-hub-chip-danger";
      case "success":
        return "work-hub-chip-success";
      default:
        return "work-hub-chip-muted";
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(card.id);
    }
  }
</script>

<div
  role="button"
  tabindex="0"
  class="work-hub-card {selected ? 'work-hub-card-selected' : ''} {compact
    ? 'work-hub-card-compact'
    : ''} {wrappingUp ? 'work-hub-card-pulse' : ''}"
  onclick={() => onSelect(card.id)}
  onkeydown={handleKeydown}
>
  <div class="flex items-start justify-between gap-2">
    <h3 class="line-clamp-2 min-w-0 text-sm font-medium leading-snug text-surface-50">
      {formatCardTitle(card)}
    </h3>
    <span class="work-hub-chip shrink-0 {chipClass(chip.tone)}">{chip.label}</span>
  </div>

  <div class="mt-2 flex min-w-0 flex-wrap gap-1">
    {#each provenance as link (link.id)}
      <button
        type="button"
        class="work-hub-provenance"
        onclick={(event) => {
          event.stopPropagation();
          onProvenance?.(link, card.id);
        }}
      >
        {link.label}
      </button>
    {/each}
  </div>
</div>
