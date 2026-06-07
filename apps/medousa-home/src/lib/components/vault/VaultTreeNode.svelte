<script lang="ts">
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";

  interface Props {
    node: VaultTreeNode;
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    depth?: number;
    onSelect: (path: string) => void;
  }

  let { node, selectedPath, labelByPath, depth = 0, onSelect }: Props = $props();

  const startsExpanded = $derived(depth < 2);
  let expanded = $state(false);
  let initialized = $state(false);

  $effect(() => {
    if (!initialized && startsExpanded) {
      expanded = true;
      initialized = true;
    }
  });

  const label = $derived(
    node.isFolder
      ? node.name
      : (node.path ? labelByPath.get(node.path) : null) ??
        vaultDisplayTitle(node.title ?? node.name, node.path),
  );

  function handleClick() {
    if (node.path && !node.isFolder) {
      onSelect(node.path);
      return;
    }
    expanded = !expanded;
  }
</script>

<div>
  <button
    type="button"
    class="flex w-full items-center gap-1 rounded-container-token px-2 py-1 text-left text-sm hover:bg-surface-800/80 {node.path ===
    selectedPath
      ? 'bg-primary-500/15 text-primary-300'
      : 'text-surface-200'}"
    style="padding-left: {8 + depth * 12}px"
    onclick={handleClick}
  >
    <span class="w-4 shrink-0 text-xs text-surface-500">
      {#if node.isFolder}
        {expanded ? "▾" : "▸"}
      {:else}
        ·
      {/if}
    </span>
    <span class="truncate">{label}</span>
  </button>

  {#if expanded}
    {#each node.children as child (child.name + (child.path ?? "folder"))}
      <VaultTreeNodeView
        node={child}
        {selectedPath}
        {labelByPath}
        depth={depth + 1}
        {onSelect}
      />
    {/each}
  {/if}
</div>
