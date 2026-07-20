<script lang="ts">
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vault } from "$lib/stores/vault.svelte";
  import { recentPathsForSpace } from "$lib/utils/vaultRecent";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";
  import VaultTreeRecentRows from "./VaultTreeRecentRows.svelte";

  interface Props {
    tree: VaultTreeNode[];
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    activeSpaceFilter?: string | null;
    onSelect: (path: string) => void;
    onMoveNote?: (sourcePath: string, targetFolderPrefix: string) => void | Promise<void>;
  }

  let { tree, selectedPath, labelByPath, activeSpaceFilter = null, onSelect, onMoveNote }: Props =
    $props();

  /** When a space is selected, skip the redundant space root row. */
  const displayNodes = $derived.by(() => {
    if (!activeSpaceFilter || tree.length !== 1) return tree;
    const root = tree[0];
    if (root.spaceId === activeSpaceFilter && root.isFolder) {
      return root.children;
    }
    return tree;
  });

  const scopedRecent = $derived(
    activeSpaceFilter
      ? recentPathsForSpace(
          vault.recentPaths,
          activeSpaceFilter,
          vault.notes,
          3,
          selectedPath,
        )
      : [],
  );
</script>

<nav class="flex-1 overflow-y-auto px-1.5 py-1" aria-label="Vault tree">
  {#if scopedRecent.length > 0}
    <VaultTreeRecentRows
      paths={scopedRecent}
      depth={0}
      {selectedPath}
      {labelByPath}
      {onSelect}
    />
  {/if}

  {#each displayNodes as node (node.name + (node.path ?? "root"))}
    <VaultTreeNodeView
      {node}
      {selectedPath}
      {labelByPath}
      {activeSpaceFilter}
      {onSelect}
      {onMoveNote}
    />
  {:else}
    {#if scopedRecent.length === 0}
      <p class="px-2 py-4 text-sm text-surface-400">No notes in vault yet.</p>
    {/if}
  {/each}
</nav>
