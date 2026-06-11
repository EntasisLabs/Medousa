<script lang="ts">
  import { onMount } from "svelte";
  import ContextMapCanvas from "$lib/components/context/ContextMapCanvas.svelte";
  import type { LocusNodeSummary } from "$lib/types/locus";
  import {
    buildContextMapGraph,
    defaultExpandedSessionIds,
    type ContextMapNode,
  } from "$lib/utils/contextMap";

  interface Props {
    nodes: LocusNodeSummary[];
    sessionLabels: Record<string, string>;
    search: string;
    loading: boolean;
    error: string | null;
    selectedNodeId?: string | null;
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
    onFocusNode,
    onClearSelection,
  }: Props = $props();

  let stageEl: HTMLDivElement | undefined = $state();
  let stageWidth = $state(960);
  let stageHeight = $state(640);
  let expandedSessionIds = $state<Set<string>>(new Set());
  let expandedBootstrapped = $state(false);

  onMount(() => {
    if (!stageEl) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const { width, height } = entry.contentRect;
      if (width > 0) stageWidth = Math.round(width);
      if (height > 0) stageHeight = Math.round(Math.max(height, 420));
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

  const graph = $derived(
    buildContextMapGraph(nodes, sessionLabels, {
      width: stageWidth,
      height: stageHeight,
      expandedSessionIds,
      searchQuery: search,
    }),
  );

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
</script>

<div class="context-map-view flex h-full min-h-0 flex-1 flex-col">
  <p class="context-map-whisper">
    {#if loading && nodes.length === 0}
      Loading link map…
    {:else if isEmpty}
      Nothing to link yet — Locus moments appear here when she stores session memory.
    {:else}
      {graph.sessionCount} session{graph.sessionCount === 1 ? "" : "s"} · {totalMoments} moment{totalMoments === 1 ? "" : "s"}
      · Hover links · click to focus · Esc or empty space clears · double-click session to expand
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
        onFocusNode={handleFocusNode}
        onClearSelection={onClearSelection}
        onToggleExpandSession={toggleExpandSession}
      />
    {/if}
  </div>
</div>
