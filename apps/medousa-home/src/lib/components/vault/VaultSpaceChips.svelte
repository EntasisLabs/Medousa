<script lang="ts">
  import {
    VAULT_FILTER_SPACES,
    countNotesBySpace,
    type VaultSpaceConfig,
  } from "$lib/config/vaultSpaces";
  import { vault } from "$lib/stores/vault.svelte";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const counts = $derived(
    countNotesBySpace(vault.notes, vault.showSystemNotes),
  );

  function chipClass(active: boolean): string {
    return active
      ? "border-primary-500/50 bg-primary-500/15 text-primary-200"
      : "border-surface-500/40 bg-surface-800/60 text-surface-300 hover:bg-surface-700/80";
  }

  function selectSpace(spaceId: string | null) {
    vault.setActiveSpaceFilter(spaceId);
  }

  function renderCount(space: VaultSpaceConfig): string {
    const count = counts.get(space.id) ?? 0;
    return count > 0 ? String(count) : "";
  }
</script>

<div
  class="flex flex-wrap gap-1.5 {compact ? '' : 'px-2 pb-2'}"
  role="tablist"
  aria-label="Vault spaces"
>
  <button
    type="button"
    role="tab"
    aria-selected={vault.activeSpaceFilter === null}
    class="inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors {chipClass(
      vault.activeSpaceFilter === null,
    )}"
    onclick={() => selectSpace(null)}
  >
    All
  </button>

  {#each VAULT_FILTER_SPACES as space (space.id)}
    {@const Icon = iconForSpace(space.id)}
    {@const count = renderCount(space)}
    <button
      type="button"
      role="tab"
      aria-selected={vault.activeSpaceFilter === space.id}
      class="inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors {chipClass(
        vault.activeSpaceFilter === space.id,
      )}"
      onclick={() => selectSpace(space.id)}
    >
      <Icon size={12} strokeWidth={2} />
      {space.label}
      {#if count}
        <span class="workshop-faint tabular-nums">{count}</span>
      {/if}
    </button>
  {/each}
</div>
