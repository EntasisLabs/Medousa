<script lang="ts">
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";

  interface Props {
    node: VaultTreeNode;
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    activeSpaceFilter?: string | null;
    depth?: number;
    onSelect: (path: string) => void;
  }

  let {
    node,
    selectedPath,
    labelByPath,
    activeSpaceFilter = null,
    depth = 0,
    onSelect,
  }: Props = $props();

  const startsExpanded = $derived(
    node.defaultCollapsed
      ? false
      : node.spaceId
        ? activeSpaceFilter === node.spaceId || activeSpaceFilter === null
        : depth < 2,
  );
  let expanded = $state(false);
  let initialized = $state(false);

  $effect(() => {
    if (!initialized && startsExpanded) {
      expanded = true;
      initialized = true;
    }
  });

  $effect(() => {
    if (activeSpaceFilter && node.spaceId === activeSpaceFilter) {
      expanded = true;
    }
  });

  const label = $derived(
    node.displayLabel ??
      (node.isFolder
        ? node.name
        : ((node.path ? labelByPath.get(node.path) : null) ??
          vaultDisplayTitle(node.title ?? node.name, node.path))),
  );

  const countHint = $derived(
    node.spaceId && node.noteCount !== undefined && node.noteCount > 0
      ? ` (${node.noteCount})`
      : "",
  );

  const SpaceIcon = $derived(
    node.spaceId ? iconForSpace(node.spaceId) : null,
  );

  const spaceActive = $derived(
    node.spaceId != null && activeSpaceFilter === node.spaceId,
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
    class="flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm hover:bg-surface-700/80 {node.path ===
    selectedPath
      ? 'bg-primary-500/15 text-primary-300'
      : spaceActive
        ? 'bg-primary-500/10 font-medium text-primary-200'
        : node.spaceId
          ? 'font-medium text-surface-100'
          : 'text-surface-200'}"
    style="padding-left: {8 + depth * 12}px"
    onclick={handleClick}
  >
    <span class="workshop-faint flex w-4 shrink-0 items-center justify-center">
      {#if node.isFolder && SpaceIcon && node.spaceId}
        <SpaceIcon size={14} strokeWidth={1.75} />
      {:else if node.isFolder}
        {expanded ? "▾" : "▸"}
      {:else}
        ·
      {/if}
    </span>
    <span class="truncate">{label}{countHint}</span>
    {#if node.path && !node.isFolder}
      <VaultKindBadge kind={node.kind} path={node.path} compact />
    {/if}
  </button>

  {#if expanded}
    {#each node.children as child (child.name + (child.path ?? "folder"))}
      <VaultTreeNodeView
        node={child}
        {selectedPath}
        {labelByPath}
        {activeSpaceFilter}
        depth={depth + 1}
        {onSelect}
      />
    {/each}
  {/if}
</div>
