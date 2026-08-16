<script lang="ts">
  import { onMount } from "svelte";
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { handleVaultNoteContextMenuEvent } from "$lib/utils/vaultContextMenuEvents";
  import { recentPathsForFolder, recentPathsForSpace } from "$lib/utils/vaultRecent";
  import {
    flattenExpandedTreeRows,
    visibleWindow,
    type VaultTreeFlatRow,
  } from "$lib/utils/vaultTreeVirtual";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";

  interface Props {
    tree: VaultTreeNode[];
    selectedPath: string | null;
    labelByPath: Map<string, string>;
    activeSpaceFilter?: string | null;
    /** Expand ancestors when selection changes. Off on mobile list so nothing stays forced open. */
    revealSelected?: boolean;
    onSelect: (path: string, event?: MouseEvent) => void;
    onMoveNote?: (sourcePath: string, targetFolderPrefix: string) => void | Promise<void>;
  }

  let {
    tree,
    selectedPath,
    labelByPath,
    activeSpaceFilter = null,
    revealSelected = true,
    onSelect,
    onMoveNote,
  }: Props = $props();

  const ROW_HEIGHT = 28;
  const OVERSCAN = 8;
  const SPACE_RECENT_KEY = "space-recent";

  /** When a space is selected, skip the redundant space root row. */
  const displayNodes = $derived.by(() => {
    if (!activeSpaceFilter || tree.length !== 1) return tree;
    const root = tree[0];
    if (root.spaceId === activeSpaceFilter && root.isFolder) {
      return root.children;
    }
    return tree;
  });

  const spaceRecentNode: VaultTreeNode = {
    name: "Recent",
    path: null,
    isFolder: true,
    dropPrefix: SPACE_RECENT_KEY,
    children: [],
    kind: undefined,
    spaceId: null,
  };

  const flatRows = $derived.by(() => {
    void vault.treeExpandedByKey;
    const knownPaths = vault.lookupSnapshot.knownPaths;
    const treeRows = flattenExpandedTreeRows(
      displayNodes,
      (key) => vault.isTreeExpanded(key) === true,
      (node) => vault.treeExpandKeyFor(node),
      (node) => {
        if (node.spaceId && !activeSpaceFilter) {
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
      },
      (key) => vault.isTreeExpanded(`recent:${key}`) === true,
    );
    if (!activeSpaceFilter) return treeRows;
    const scoped = recentPathsForSpace(
      vault.recentPaths,
      activeSpaceFilter,
      vault.notes,
      3,
      selectedPath,
    );
    if (scoped.length === 0) return treeRows;
    const leading: VaultTreeFlatRow[] = [
      {
        id: `recent-header:${SPACE_RECENT_KEY}:0`,
        node: spaceRecentNode,
        depth: 0,
        recentHeader: true,
      },
    ];
    if (vault.isTreeExpanded(`recent:${SPACE_RECENT_KEY}`) === true) {
      for (const path of scoped) {
        leading.push({
          id: `recent:${path}:0`,
          node: spaceRecentNode,
          depth: 1,
          recentPath: path,
        });
      }
    }
    return [...leading, ...treeRows];
  });

  let scrollEl: HTMLElement | null = $state(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(400);

  const windowed = $derived(
    visibleWindow(flatRows.length, scrollTop, viewportHeight, ROW_HEIGHT, OVERSCAN),
  );
  const visibleRows = $derived(flatRows.slice(windowed.start, windowed.end));

  onMount(() => {
    function onKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (vault.selectedPaths.size === 0) return;
      event.preventDefault();
      vault.clearRailSelection();
    }
    window.addEventListener("keydown", onKeydown);

    const el = scrollEl;
    let ro: ResizeObserver | undefined;
    if (el && typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver((entries) => {
        const entry = entries[0];
        if (entry) viewportHeight = entry.contentRect.height;
      });
      ro.observe(el);
      viewportHeight = el.clientHeight;
    }

    return () => {
      window.removeEventListener("keydown", onKeydown);
      ro?.disconnect();
    };
  });

  function onScroll() {
    if (scrollEl) scrollTop = scrollEl.scrollTop;
  }

  function recentExpandKey(row: VaultTreeFlatRow): string {
    return `recent:${vault.treeExpandKeyFor(row.node)}`;
  }
</script>

<nav
  class="flex-1 overflow-y-auto px-1.5 py-1"
  aria-label="Vault tree"
  bind:this={scrollEl}
  onscroll={onScroll}
>
  {#if flatRows.length === 0}
    <p class="px-2 py-4 text-sm text-content-tertiary">No notes in vault yet.</p>
  {:else}
    <div style="height: {windowed.totalHeight}px; position: relative;">
      <div style="transform: translateY({windowed.offsetY}px);">
        {#each visibleRows as row (row.id)}
          <div style="height: {ROW_HEIGHT}px;">
            {#if row.recentHeader}
              <button
                type="button"
                class="vault-tree-row flex h-full w-full items-center gap-1.5 rounded-container-token px-2 text-left text-xs text-content-tertiary outline-none hover:bg-surface-700/60 hover:text-surface-200"
                style="padding-left: {8 + row.depth * 12}px"
                aria-expanded={vault.isTreeExpanded(recentExpandKey(row)) === true}
                onclick={() =>
                  vault.setTreeExpanded(
                    recentExpandKey(row),
                    vault.isTreeExpanded(recentExpandKey(row)) !== true,
                  )}
              >
                <span class="workshop-faint flex w-4 shrink-0 items-center justify-center">
                  {vault.isTreeExpanded(recentExpandKey(row)) === true ? "▾" : "▸"}
                </span>
                <span class="min-w-0 flex-1 truncate">Recent</span>
              </button>
            {:else if row.recentPath}
              <button
                type="button"
                class="vault-tree-row flex h-full w-full items-center gap-1.5 rounded-container-token px-2 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {vault.isRailPathSelected(
                  row.recentPath,
                )
                  ? 'bg-primary-500/15 text-content-link'
                  : 'text-content-secondary'}"
                style="padding-left: {8 + row.depth * 12}px"
                title={row.recentPath}
                onclick={(event) => onSelect(row.recentPath!, event)}
                oncontextmenu={(event) =>
                  handleVaultNoteContextMenuEvent(row.recentPath!, event)}
              >
                <span class="w-4 shrink-0"></span>
                <span class="min-w-0 flex-1 truncate">
                  {labelByPath.get(row.recentPath) ??
                    vaultDisplayTitle(row.recentPath, row.recentPath)}
                </span>
              </button>
            {:else}
              <VaultTreeNodeView
                node={row.node}
                {selectedPath}
                {labelByPath}
                {activeSpaceFilter}
                {revealSelected}
                depth={row.depth}
                virtualized={true}
                {onSelect}
                {onMoveNote}
              />
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</nav>
