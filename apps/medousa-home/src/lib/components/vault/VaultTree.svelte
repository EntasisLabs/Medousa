<script lang="ts">
  import { onMount } from "svelte";
  import type { VaultTreeNode } from "$lib/types/vault";
  import { vault } from "$lib/stores/vault.svelte";
  import { recentPathsForSpace } from "$lib/utils/vaultRecent";
  import {
    flattenExpandedTreeRows,
    visibleWindow,
  } from "$lib/utils/vaultTreeVirtual";
  import VaultTreeNodeView from "./VaultTreeNode.svelte";
  import VaultTreeRecentRows from "./VaultTreeRecentRows.svelte";

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

  const flatRows = $derived.by(() => {
    void vault.treeExpandedByKey;
    return flattenExpandedTreeRows(
      displayNodes,
      (key) => vault.isTreeExpanded(key) === true,
      (node) => vault.treeExpandKeyFor(node),
    );
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
</script>

<nav
  class="flex-1 overflow-y-auto px-1.5 py-1"
  aria-label="Vault tree"
  bind:this={scrollEl}
  onscroll={onScroll}
>
  {#if scopedRecent.length > 0}
    <VaultTreeRecentRows
      paths={scopedRecent}
      depth={0}
      {selectedPath}
      {labelByPath}
      {onSelect}
    />
  {/if}

  {#if flatRows.length === 0}
    {#if scopedRecent.length === 0}
      <p class="px-2 py-4 text-sm text-content-tertiary">No notes in vault yet.</p>
    {/if}
  {:else}
    <div style="height: {windowed.totalHeight}px; position: relative;">
      <div style="transform: translateY({windowed.offsetY}px);">
        {#each visibleRows as row (row.id)}
          <div style="min-height: {ROW_HEIGHT}px;">
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
          </div>
        {/each}
      </div>
    </div>
  {/if}
</nav>
