<script lang="ts">
  import type { ProvenanceChip } from "$lib/utils/workHub";

  interface Props {
    chips: ProvenanceChip[];
    onVault?: (path: string) => void;
    onChat?: () => void;
    onTools?: () => void;
  }

  let { chips, onVault, onChat, onTools }: Props = $props();

  function handleChip(chip: ProvenanceChip) {
    if (chip.kind === "vault" && chip.href && onVault) {
      onVault(chip.href);
      return;
    }
    if (chip.kind === "chat" && onChat) {
      onChat();
      return;
    }
    if (chip.kind === "tools" && onTools) {
      onTools();
    }
  }
</script>

{#if chips.length > 0}
  <div class="manifest-provenance flex min-w-0 flex-wrap gap-1.5">
    {#each chips as chip (chip.id)}
      <button
        type="button"
        class="work-hub-provenance"
        onclick={() => handleChip(chip)}
      >
        {chip.label}
      </button>
    {/each}
  </div>
{/if}
