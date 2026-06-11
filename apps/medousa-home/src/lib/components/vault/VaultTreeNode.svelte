<script lang="ts">
  import { GripVertical } from "@lucide/svelte";
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { isVaultPointerDragging, shouldSuppressVaultTreeClick, startVaultPointerDrag } from "$lib/utils/vaultTreeDrag";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";

  interface Props {
    node: VaultTreeNode;
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    activeSpaceFilter?: string | null;
    depth?: number;
    onSelect: (path: string) => void;
    onMoveNote?: (sourcePath: string, targetFolderPrefix: string) => void | Promise<void>;
  }

  let {
    node,
    selectedPath,
    labelByPath,
    activeSpaceFilter = null,
    depth = 0,
    onSelect,
    onMoveNote,
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

  const isNoteLeaf = $derived(Boolean(node.path && !node.isFolder));

  const dropPrefix = $derived.by(() => {
    if (node.dropPrefix) return node.dropPrefix;
    if (node.path && !node.isFolder) {
      const parts = node.path.split("/").filter(Boolean);
      if (parts.length <= 1) return null;
      parts.pop();
      return `${parts.join("/")}/`;
    }
    return null;
  });

  const isDropTarget = $derived(Boolean(onMoveNote && dropPrefix));

  function handleClick() {
    if (shouldSuppressVaultTreeClick()) return;
    if (node.path && !node.isFolder) {
      onSelect(node.path);
      return;
    }
    expanded = !expanded;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      handleClick();
    }
  }

  function handlePointerEnter() {
    if (isVaultPointerDragging() && node.isFolder && !expanded) {
      expanded = true;
    }
  }

  function handleGripPointerDown(event: PointerEvent) {
    if (!isNoteLeaf || !node.path || !onMoveNote) return;
    event.preventDefault();
    event.stopPropagation();
    startVaultPointerDrag(node.path, (sourcePath, targetPrefix) => {
      void onMoveNote(sourcePath, targetPrefix);
    }, event);
  }
</script>

<div>
  <div
    role="treeitem"
    aria-selected={node.path === selectedPath}
    tabindex="0"
    class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {node.path ===
    selectedPath
      ? 'bg-primary-500/15 text-primary-300'
      : spaceActive
        ? 'bg-primary-500/10 font-medium text-primary-200'
        : node.spaceId
          ? 'font-medium text-surface-100'
          : 'text-surface-200'} {isDropTarget ? 'vault-tree-drop-target' : ''} cursor-pointer"
    style="padding-left: {8 + depth * 12}px"
    data-vault-drop-prefix={isDropTarget ? dropPrefix : undefined}
    onclick={handleClick}
    onkeydown={handleKeydown}
    onpointerenter={handlePointerEnter}
  >
    {#if isNoteLeaf}
      <button
        type="button"
        class="vault-tree-drag-handle workshop-faint flex w-4 shrink-0 cursor-grab items-center justify-center border-0 bg-transparent p-0 active:cursor-grabbing"
        title="Drag to move"
        aria-label="Drag to move note"
        onpointerdown={handleGripPointerDown}
      >
        <GripVertical size={12} strokeWidth={2} />
      </button>
    {:else}
      <span class="workshop-faint flex w-4 shrink-0 items-center justify-center">
        {#if node.isFolder && SpaceIcon && node.spaceId}
          <SpaceIcon size={14} strokeWidth={1.75} />
        {:else if node.isFolder}
          {expanded ? "▾" : "▸"}
        {:else}
          ·
        {/if}
      </span>
    {/if}
    <span class="min-w-0 flex-1 truncate">{label}{countHint}</span>
    {#if node.path && !node.isFolder}
      <VaultKindBadge kind={node.kind} path={node.path} compact />
    {/if}
  </div>

  {#if expanded}
    {#each node.children as child (child.name + (child.path ?? "folder"))}
      <VaultTreeNodeView
        node={child}
        {selectedPath}
        {labelByPath}
        {activeSpaceFilter}
        depth={depth + 1}
        {onSelect}
        {onMoveNote}
      />
    {/each}
  {/if}
</div>
