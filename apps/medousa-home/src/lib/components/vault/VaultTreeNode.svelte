<script lang="ts">
  import { Folder, FolderOpen, GripVertical } from "@lucide/svelte";
  import type { Component } from "svelte";
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    bindVaultLongPress,
    handleVaultFolderContextMenuEvent,
    handleVaultNoteContextMenuEvent,
    shouldSuppressVaultContextMenuClick,
  } from "$lib/utils/vaultContextMenuEvents";
  import {
    isVaultPointerDragging,
    shouldSuppressVaultTreeClick,
    startVaultPointerDrag,
  } from "$lib/utils/vaultTreeDrag";
  import { recentPathsForFolder, recentPathsForSpace } from "$lib/utils/vaultRecent";
  import { vault } from "$lib/stores/vault.svelte";
  import {
    folderIconStorageKey,
    vaultFolderIcons,
  } from "$lib/stores/vaultFolderIcons.svelte";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";
  import VaultTreeRecentRows from "./VaultTreeRecentRows.svelte";
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

  function treeNodeContainsPath(node: VaultTreeNode, path: string | null): boolean {
    if (!path) return false;
    if (node.path === path) return true;
    return node.children.some((child) => treeNodeContainsPath(child, path));
  }

  const startsExpanded = $derived(
    node.defaultCollapsed
      ? false
      : treeNodeContainsPath(node, selectedPath) ||
          (activeSpaceFilter != null && node.spaceId === activeSpaceFilter),
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
    if (selectedPath && treeNodeContainsPath(node, selectedPath)) {
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

  const isNoteLeaf = $derived(Boolean(node.path && !node.isFolder));
  const isPathFolder = $derived(Boolean(node.isFolder && !node.spaceId));

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

  const iconKey = $derived(
    node.isFolder
      ? folderIconStorageKey({
          dropPrefix,
          spaceId: node.spaceId,
        })
      : null,
  );

  const customIconName = $derived.by(() => {
    // Touch the map so icon changes re-render this row.
    const icons = vaultFolderIcons.icons;
    if (!iconKey) return null;
    return icons[iconKey] ?? null;
  });

  const FolderIcon = $derived.by((): Component => {
    if (customIconName) return environmentIcon(customIconName);
    if (node.spaceId) return iconForSpace(node.spaceId);
    return expanded ? FolderOpen : Folder;
  });

  const spaceActive = $derived(
    node.spaceId != null && activeSpaceFilter === node.spaceId,
  );

  const isDropTarget = $derived(Boolean(onMoveNote && dropPrefix));

  const knownPaths = $derived(new Set(vault.notes.map((note) => note.path)));

  const recentRows = $derived.by(() => {
    if (!node.isFolder) return [];
    if (node.spaceId && activeSpaceFilter) return [];
    if (node.spaceId) {
      return recentPathsForSpace(
        vault.recentPaths,
        node.spaceId,
        vault.notes,
        3,
        selectedPath,
      );
    }
    if (node.dropPrefix) {
      return recentPathsForFolder(
        vault.recentPaths,
        node.dropPrefix,
        knownPaths,
        3,
        selectedPath,
      );
    }
    return [];
  });

  const nestInset = $derived(8 + depth * 12 + 8);

  function handleClick() {
    if (shouldSuppressVaultTreeClick() || shouldSuppressVaultContextMenuClick()) return;
    if (node.path && !node.isFolder) {
      onSelect(node.path);
      return;
    }
    expanded = !expanded;
  }

  function handleContextMenu(event: MouseEvent) {
    if (isNoteLeaf && node.path) {
      handleVaultNoteContextMenuEvent(node.path, event);
      return;
    }
    if (node.isFolder && iconKey) {
      handleVaultFolderContextMenuEvent(iconKey, label, event, node.spaceId);
    }
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

<div
  class="vault-tree-node"
  class:vault-tree-node--folder={node.isFolder}
  class:vault-tree-node--path-folder={isPathFolder}
  class:vault-tree-node--open={expanded && node.isFolder}
  class:vault-tree-node--space={Boolean(node.spaceId)}
>
  <div
    role="treeitem"
    aria-selected={node.path === selectedPath}
    aria-expanded={node.isFolder ? expanded : undefined}
    tabindex="0"
    class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {node.path ===
    selectedPath
      ? 'bg-primary-500/15 text-primary-300'
      : spaceActive
        ? 'bg-primary-500/10 font-medium text-primary-200'
        : node.spaceId
          ? 'font-medium text-surface-100'
          : isPathFolder
            ? 'vault-tree-row--folder text-surface-100'
            : 'vault-tree-row--note text-surface-300'} {expanded && isPathFolder
      ? 'vault-tree-row--folder-open'
      : ''} {isDropTarget ? 'vault-tree-drop-target' : ''} cursor-pointer"
    style="padding-left: {8 + depth * 12}px"
    data-vault-drop-prefix={isDropTarget ? dropPrefix : undefined}
    onclick={handleClick}
    onkeydown={handleKeydown}
    onpointerenter={handlePointerEnter}
    oncontextmenu={handleContextMenu}
    use:bindVaultLongPress={() => (isNoteLeaf && node.path ? node.path : null)}
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
      <span class="vault-tree-folder-icon flex w-4 shrink-0 items-center justify-center">
        <FolderIcon size={14} strokeWidth={1.85} />
      </span>
    {/if}
    <span class="min-w-0 flex-1 truncate">{label}{countHint}</span>
    {#if node.path && !node.isFolder}
      <VaultKindBadge kind={node.kind} path={node.path} compact />
    {:else if isPathFolder && node.children.length > 0}
      <span class="vault-tree-folder-count">{node.children.length}</span>
    {/if}
  </div>

  {#if expanded}
    <div
      class="vault-tree-nest"
      class:vault-tree-nest--path={isPathFolder}
      class:vault-tree-nest--space={Boolean(node.spaceId)}
      style:--vault-tree-nest-inset="{nestInset}px"
      role="group"
      aria-label="{label} contents"
    >
      <VaultTreeRecentRows
        paths={recentRows}
        depth={depth}
        {selectedPath}
        {labelByPath}
        {onSelect}
      />
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
    </div>
  {/if}
</div>
