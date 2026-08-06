<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import ContextMapCanvas from "$lib/components/context/ContextMapCanvas.svelte";
  import MapMomentOverlay from "$lib/components/context/MapMomentOverlay.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import type { LocusNodeDetailResponse, LocusNodeSummary } from "$lib/types/locus";
  import type { VaultNote } from "$lib/types/vault";
  import {
    applySimulationPositions,
    buildContextMapGraph,
    defaultExpandedSessionIds,
    type ContextMapDensity,
    type ContextMapNode,
  } from "$lib/utils/contextMap";
  import { createContextMapSimulation } from "$lib/utils/contextMapPhysics";

  interface Props {
    nodes: LocusNodeSummary[];
    vaultNotes?: VaultNote[];
    sessionLabels: Record<string, string>;
    search: string;
    loading: boolean;
    error: string | null;
    selectedNodeId?: string | null;
    density?: ContextMapDensity;
    momentDetail?: LocusNodeDetailResponse | null;
    momentDetailLoading?: boolean;
    chatSessionAvailable?: boolean;
    onFocusNode?: (node: ContextMapNode) => void;
    onClearSelection?: () => void;
    onOpenChat?: () => void;
  }

  let {
    nodes,
    vaultNotes = [],
    sessionLabels,
    search,
    loading,
    error,
    selectedNodeId = null,
    density = "default",
    momentDetail = null,
    momentDetailLoading = false,
    chatSessionAvailable = false,
    onFocusNode,
    onClearSelection,
    onOpenChat,
  }: Props = $props();

  let stageEl: HTMLDivElement | undefined = $state();
  let stageWidth = $state(960);
  let stageHeight = $state(640);
  let expandedSessionIds = $state<Set<string>>(new Set());
  let expandedBootstrapped = $state(false);

  const simulation = createContextMapSimulation();
  let livePositions = $state(new Map<string, { x: number; y: number }>());
  let simRaf = 0;
  let lastTopologyKey = "";

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

  onDestroy(() => {
    if (simRaf) cancelAnimationFrame(simRaf);
    simulation.dispose();
  });

  $effect(() => {
    nodes;
    search;
    if (expandedBootstrapped || nodes.length === 0 || search.trim()) return;
    expandedSessionIds = defaultExpandedSessionIds(nodes);
    expandedBootstrapped = true;
  });

  $effect(() => {
    const nonce = contextShell.mapExpandNonce;
    const sessionId = contextShell.mapExpandSessionId;
    if (!nonce || !sessionId) return;
    const next = new Set(expandedSessionIds);
    next.add(sessionId);
    expandedSessionIds = next;
  });

  /** Topology only — positions come from the settle simulation, not this rebuild. */
  const baseGraph = $derived(
    buildContextMapGraph(nodes, sessionLabels, {
      width: stageWidth,
      height: stageHeight,
      expandedSessionIds,
      searchQuery: search,
      density,
      vaultNotes,
      avecMins: contextShell.mapAvecMins,
    }),
  );

  function topologyKey(graph: typeof baseGraph): string {
    const nodeIds = graph.nodes
      .filter((node) => node.visible)
      .map((node) => node.id)
      .sort()
      .join("|");
    const edgeIds = graph.edges
      .filter((edge) => edge.visible)
      .map((edge) => edge.id)
      .sort()
      .join("|");
    return `${graph.width}x${graph.height}:${nodeIds}#${edgeIds}`;
  }

  function publishPositions() {
    livePositions = simulation.getPositions();
  }

  function ensureSimLoop() {
    if (simRaf) return;
    const step = () => {
      const awake = simulation.tick();
      publishPositions();
      if (awake) {
        simRaf = requestAnimationFrame(step);
      } else {
        simRaf = 0;
      }
    };
    simRaf = requestAnimationFrame(step);
  }

  $effect(() => {
    const graph = baseGraph;
    if (graph.nodes.length === 0) {
      lastTopologyKey = "";
      return;
    }

    const key = topologyKey(graph);
    if (key === lastTopologyKey) return;
    lastTopologyKey = key;

    const prior = simulation.getPositions();
    simulation.setTopology(
      graph.nodes
        .filter((node) => node.visible)
        .map((node) => {
          const kept = prior.get(node.id);
          return {
            id: node.id,
            kind: node.kind,
            radius: node.radius,
            weight: node.weight,
            x: kept?.x ?? node.x,
            y: kept?.y ?? node.y,
          };
        }),
      graph.edges
        .filter((edge) => edge.visible)
        .map((edge) => ({
          id: edge.id,
          from: edge.from,
          to: edge.to,
          kind: edge.kind,
          strength: edge.strength,
          ghost: edge.renderMode === "ghost",
        })),
      graph.width,
      graph.height,
    );
    publishPositions();
    ensureSimLoop();
  });

  const graph = $derived(applySimulationPositions(baseGraph, livePositions));

  const isEmpty = $derived(!loading && graph.nodes.length === 0);
  const totalMoments = $derived(new Set(nodes.map((node) => node.sync_key)).size);
  const noteCount = $derived(graph.nodes.filter((node) => node.kind === "note").length);

  const selectedSyncKey = $derived(
    selectedNodeId?.startsWith("thread:")
      ? selectedNodeId.slice("thread:".length)
      : null,
  );
  const overlayDetail = $derived.by(() => {
    if (!selectedSyncKey) return null;
    if (momentDetail?.node.sync_key === selectedSyncKey) return momentDetail;
    return null;
  });

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

  function handleDragBegin(nodeId: string, x: number, y: number) {
    simulation.pin(nodeId, x, y);
    simulation.restart({ alpha: 0.22 });
    publishPositions();
    ensureSimLoop();
  }

  function handleDragMove(nodeId: string, x: number, y: number) {
    simulation.pin(nodeId, x, y);
    publishPositions();
    ensureSimLoop();
  }

  function handleDragEnd(nodeId: string) {
    simulation.unpin(nodeId);
    // Gentle settle — a hard kick re-collapses the layout you just made.
    simulation.restart({ alpha: 0.2 });
    publishPositions();
    ensureSimLoop();
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
      Nothing to link yet — Locus moments and vault notes appear here as they accumulate.
    {:else}
      {graph.sessionCount} session{graph.sessionCount === 1 ? "" : "s"} · {totalMoments} moment{totalMoments === 1 ? "" : "s"}{#if noteCount > 0}
        · {noteCount} note{noteCount === 1 ? "" : "s"}{/if}
    {/if}
  </p>

  <div bind:this={stageEl} class="context-map-stage">
    {#if error}
      <p class="absolute inset-0 flex items-center justify-center px-4 text-sm text-content-warning">
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
        onDragBegin={handleDragBegin}
        onDragMove={handleDragMove}
        onDragEnd={handleDragEnd}
      />
    {/if}

    {#if selectedSyncKey && overlayDetail}
      <div class="context-map-moment-overlay">
        <MapMomentOverlay
          detail={overlayDetail}
          {chatSessionAvailable}
          onOpenChat={onOpenChat}
          onClear={onClearSelection}
        />
      </div>
    {:else if selectedSyncKey && momentDetailLoading}
      <div class="context-map-moment-overlay">
        <p class="workshop-muted px-3 py-4 text-sm">Loading this moment…</p>
      </div>
    {/if}
  </div>
</div>
