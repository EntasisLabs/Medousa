<script lang="ts">
  import type { VaultTreeNode } from "$lib/types/vault";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";

  interface Props {
    tree: VaultTreeNode[];
    selectedPath: string | null;
    onSelect: (path: string) => void;
  }

  let { tree, selectedPath, onSelect }: Props = $props();
</script>

<nav class="flex-1 overflow-y-auto p-2" aria-label="Vault tree">
  {#each tree as node (node.name + (node.path ?? "root"))}
    <VaultTreeNodeView {node} {selectedPath} {onSelect} />
  {:else}
    <p class="px-2 py-4 text-sm text-surface-400">No notes in vault yet.</p>
  {/each}
</nav>
