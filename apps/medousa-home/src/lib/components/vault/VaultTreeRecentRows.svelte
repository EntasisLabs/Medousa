<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    paths: string[];
    depth: number;
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    onSelect: (path: string) => void;
  }

  let { paths, depth, selectedPath, labelByPath, onSelect }: Props = $props();

  let expanded = $state(false);

  const visible = $derived(paths.filter((path) => path !== selectedPath));
</script>

{#if visible.length > 0}
  <div>
    <button
      type="button"
      class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-xs text-surface-400 outline-none hover:bg-surface-700/60 hover:text-surface-200"
      style="padding-left: {8 + depth * 12}px"
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      <span class="workshop-faint flex w-4 shrink-0 items-center justify-center">
        {expanded ? "▾" : "▸"}
      </span>
      <span class="min-w-0 flex-1 truncate">Recent</span>
      <span class="tabular-nums text-surface-500">{visible.length}</span>
    </button>

    {#if expanded}
      {#each visible as path (path)}
        {@const note = vault.notes.find((entry) => entry.path === path)}
        <button
          type="button"
          class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {path ===
          selectedPath
            ? 'bg-primary-500/15 text-primary-300'
            : 'text-surface-300'}"
          style="padding-left: {8 + (depth + 1) * 12}px"
          title={path}
          onclick={() => onSelect(path)}
        >
          <span class="w-4 shrink-0"></span>
          <span class="min-w-0 flex-1 truncate">
            {labelByPath.get(path) ?? vaultDisplayTitle(note?.title ?? path, path)}
          </span>
        </button>
      {/each}
    {/if}
  </div>
{/if}
