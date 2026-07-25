<script lang="ts">
  import { onMount } from "svelte";
  import ContextMapCanvas from "$lib/components/context/ContextMapCanvas.svelte";
  import type { LocusNodeSummary } from "$lib/types/locus";
  import {
    applyPinnedPositions,
    buildContextMapGraph,
    defaultExpandedSessionIds,
    type ContextMapDensity,
    type ContextMapNode,
  } from "$lib/utils/contextMap";

  interface Props {
    nodes: LocusNodeSummary[];
    sessionLabels: Record<string, string>;
    search: string;
    loading: boolean;
    error: string | null;
    selectedNodeId?: string | null;
    density?: ContextMapDensity;
    onFocusNode?: (node: ContextMapNode) => void;
    onClearSelection?: () => void;
  }

  let {
    nodes,
    sessionLabels,
    search,
    loading,
    error,
    selectedNodeId = null,
    density = "default",
    onFocusNode,
    onClearSelection,
  }: Props = $props();

  let stageEl: HTMLDivElement | undefined = $state();
  let stageWidth = $state(density === "rail" ? 320 : 960);
  let stageHeight = $state(density === "rail" ? 480 : 640);
  let expandedSessionIds = $state<Set<string>>(new Set());
  let expandedBootstrapped = $state(false);
  let pinnedPositions = $state(new Map<string, { x: number; y: number }>());

  onMount(() => {
    if (!stageEl) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const { width, height } = entry.contentRect;
      if (width > 0) stageWidth = Math.round(width);
      if (height > 0) {
        const floor = density === "rail" ? 280 : 420;
        stageHeight = Math.round(Math.max(height, floor));
      }
    });
    observer.observe(stageEl);
    return () => observer.disconnect();
  });

  $effect(() => {
    nodes;
    search;
    if (expandedBootstrapped || nodes.length === 0 || search.trim()) return;
    expandedSessionIds = defaultExpandedSessionIds(nodes);
    expandedBootstrapped = true;
  });

  /** Layout only — never depends on pins, so drags don't re-run force simulation. */
  const baseGraph = $derived(
    buildContextMapGraph(nodes, sessionLabels, {
      width: stageWidth,
      height: stageHeight,
      expandedSessionIds,
      searchQuery: search,
      density,
    }),
  );

  const graph = $derived(applyPinnedPositions(baseGraph, pinnedPositions));

  const isEmpty = $derived(!loading && graph.sessionCount === 0);
  const totalMoments = $derived(new Set(nodes.map((node) => node.sync_key)).size);

  function toggleExpandSession(sessionId: string) {
    const next = new Set(expandedSessionIds);
    if (next.has(sessionId)) {
      next.delete(sessionId);
    } else {
      next.add(sessionId);
    }
    expandedSessionIds = next;
  }

  function handleFocusNode(node: ContextMapNode) {
    onFocusNode?.(node);
  }

  function handlePinNode(nodeId: string, x: number, y: number) {
    const next = new Map(pinnedPositions);
    next.set(nodeId, { x, y });
    pinnedPositions = next;
  }
</script>

<div
  class="context-map-view flex h-full min-h-0 flex-1 flex-col"
  class:context-map-view-rail={density === "rail"}
>
  <p
    class="context-map-whisper"
    title="Hover links · click to focus · drag nodes to rearrange · Esc or empty space clears · double-click session to expand"
  >
    {#if loading && nodes.length === 0}
      Loading link map…
    {:else if isEmpty}
      Nothing to link yet — Locus moments appear here when she stores session memory.
    {:else}
      {graph.sessionCount} session{graph.sessionCount === 1 ? "" : "s"} · {totalMoments} moment{totalMoments === 1 ? "" : "s"}
    {/if}
  </p>

  <div bind:this={stageEl} class="context-map-stage">
    {#if error}
      <p class="absolute inset-0 flex items-center justify-center px-4 text-sm text-warning-400">
        {error}
      </p>
    {:else if !isEmpty}
      <ContextMapCanvas
        {graph}
        {search}
        {selectedNodeId}
        {density}
        onFocusNode={handleFocusNode}
        onClearSelection={onClearSelection}
        onToggleExpandSession={toggleExpandSession}
        onPinNode={handlePinNode}
      />
    {/if}
  </div>
</div>
