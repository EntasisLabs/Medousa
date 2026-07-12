<script lang="ts">
  import { onMount } from "svelte";
  import {
    boundsForNodeIds,
    graphBounds,
    mapNeighborhood,
    neighborSummary,
    type ContextMapGraph,
    type ContextMapNode,
  } from "$lib/utils/contextMap";
  import { MAP_KIND_LEGEND, mapNodeStyleVars, resolveMapLabelMode, mapDisplayLabel } from "$lib/utils/contextMapVisual";

  interface Props {
    graph: ContextMapGraph;
    search?: string;
    selectedNodeId?: string | null;
    onFocusNode?: (node: ContextMapNode) => void;
    onClearSelection?: () => void;
    onToggleExpandSession?: (sessionId: string) => void;
  }

  let {
    graph,
    search = "",
    selectedNodeId = null,
    onFocusNode,
    onClearSelection,
    onToggleExpandSession,
  }: Props = $props();

  let viewportEl: HTMLDivElement | undefined = $state();
  let panX = $state(0);
  let panY = $state(0);
  let zoom = $state(1);
  let hoveredNodeId = $state<string | null>(null);
  let dragging = $state(false);
  let suppressClick = $state(false);
  let dragOrigin = { x: 0, y: 0, panX: 0, panY: 0 };
  let lastFitKey = $state("");
  let lastClick = { id: "", time: 0 };
  let animFrame = 0;
  let pinching = $state(false);
  let pinchStartDistance = 0;
  let pinchStartZoom = 1;

  const nodeById = $derived(Object.fromEntries(graph.nodes.map((node) => [node.id, node])));
  const visibleNodes = $derived(graph.nodes.filter((node) => node.visible));
  const visibleEdges = $derived(
    graph.edges
      .filter(
        (edge) => edge.visible && nodeById[edge.from]?.visible && nodeById[edge.to]?.visible,
      )
      .sort((left, right) => edgeSort(left.kind) - edgeSort(right.kind)),
  );

  const focusNodeId = $derived(hoveredNodeId ?? selectedNodeId);
  const neighborhood = $derived(mapNeighborhood(graph, focusNodeId));
  const focusActive = $derived(Boolean(focusNodeId));
  const selectionActive = $derived(Boolean(selectedNodeId));
  const hoverPreview = $derived(
    hoveredNodeId ? (nodeById[hoveredNodeId] ?? null) : null,
  );
  const selectedPreview = $derived(
    selectedNodeId ? (nodeById[selectedNodeId] ?? null) : null,
  );

  function edgeSort(kind: string): number {
    if (kind === "session_chain") return 0;
    if (kind === "membership") return 1;
    return 2;
  }

  function viewportForBounds(
    bounds: { minX: number; minY: number; maxX: number; maxY: number },
    pad = 56,
    maxScale = 1.35,
  ) {
    if (!viewportEl) return null;
    const { clientWidth, clientHeight } = viewportEl;
    const boxW = bounds.maxX - bounds.minX;
    const boxH = bounds.maxY - bounds.minY;
    const scale = Math.min(
      (clientWidth - pad * 2) / boxW,
      (clientHeight - pad * 2) / boxH,
      maxScale,
    );
    const nextZoom = Math.max(0.2, scale);
    return {
      zoom: nextZoom,
      panX: clientWidth / 2 - ((bounds.minX + bounds.maxX) / 2) * nextZoom,
      panY: clientHeight / 2 - ((bounds.minY + bounds.maxY) / 2) * nextZoom,
    };
  }

  function applyViewport(next: { panX: number; panY: number; zoom: number }) {
    panX = next.panX;
    panY = next.panY;
    zoom = next.zoom;
  }

  function animateViewport(
    target: { panX: number; panY: number; zoom: number },
    duration = 420,
  ) {
    cancelAnimationFrame(animFrame);
    const start = { panX, panY, zoom };
    const startTime = performance.now();

    const tick = (now: number) => {
      const t = Math.min(1, (now - startTime) / duration);
      const ease = 1 - (1 - t) ** 3;
      panX = start.panX + (target.panX - start.panX) * ease;
      panY = start.panY + (target.panY - start.panY) * ease;
      zoom = start.zoom + (target.zoom - start.zoom) * ease;
      if (t < 1) animFrame = requestAnimationFrame(tick);
    };

    animFrame = requestAnimationFrame(tick);
  }

  function fitToView(animate = false) {
    const bounds = graphBounds(graph);
    if (!bounds) return;
    const next = viewportForBounds(bounds);
    if (!next) return;
    if (animate) animateViewport(next);
    else applyViewport(next);
  }

  function flyToNeighborhood(nodeId: string) {
    const ids = mapNeighborhood(graph, nodeId);
    const bounds = boundsForNodeIds(graph, ids);
    if (!bounds) return;
    const next = viewportForBounds(bounds, 72, 1.85);
    if (!next) return;
    animateViewport(next);
  }

  function clearSelection(animateCamera = true) {
    hoveredNodeId = null;
    suppressClick = false;
    onClearSelection?.();
    if (animateCamera) fitToView(true);
  }

  function isMapChromeTarget(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return Boolean(
      target.closest(
        "[data-map-node], .context-map-controls, .context-map-hover-card, .context-map-legend, .context-map-clear-link",
      ),
    );
  }

  $effect(() => {
    if (!selectedNodeId) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      clearSelection(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  onMount(() => {
    fitToView();
  });

  $effect(() => {
    const fitKey = `${graph.sessionCount}:${search.trim()}`;
    if (fitKey === lastFitKey) return;
    lastFitKey = fitKey;
    fitToView();
  });

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || pinching) return;
    if (isMapChromeTarget(event.target)) return;
    dragging = true;
    suppressClick = false;
    dragOrigin = { x: event.clientX, y: event.clientY, panX, panY };
    viewportEl?.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return;
    const dx = event.clientX - dragOrigin.x;
    const dy = event.clientY - dragOrigin.y;
    if (Math.hypot(dx, dy) > 4) suppressClick = true;
    panX = dragOrigin.panX + dx;
    panY = dragOrigin.panY + dy;
  }

  function onPointerUp(event: PointerEvent) {
    if (!dragging) return;
    const shouldClear = !suppressClick && Boolean(selectedNodeId) && !isMapChromeTarget(event.target);
    dragging = false;
    viewportEl?.releasePointerCapture(event.pointerId);
    if (shouldClear) clearSelection(true);
  }

  function onWheel(event: WheelEvent) {
    event.preventDefault();
    if (!viewportEl) return;
    const rect = viewportEl.getBoundingClientRect();
    const mx = event.clientX - rect.left;
    const my = event.clientY - rect.top;
    const factor = event.deltaY > 0 ? 0.9 : 1.1;
    applyZoomAt(mx, my, zoom * factor);
  }

  function zoomBy(factor: number) {
    if (!viewportEl) return;
    const mx = viewportEl.clientWidth / 2;
    const my = viewportEl.clientHeight / 2;
    applyZoomAt(mx, my, zoom * factor);
  }

  function applyZoomAt(focalX: number, focalY: number, nextZoom: number) {
    const clamped = Math.min(3.5, Math.max(0.18, nextZoom));
    panX = focalX - (focalX - panX) * (clamped / zoom);
    panY = focalY - (focalY - panY) * (clamped / zoom);
    zoom = clamped;
  }

  function touchDistance(touches: TouchList): number {
    if (touches.length < 2) return 0;
    const dx = touches[0].clientX - touches[1].clientX;
    const dy = touches[0].clientY - touches[1].clientY;
    return Math.hypot(dx, dy);
  }

  function touchCenter(touches: TouchList, rect: DOMRect) {
    return {
      x: (touches[0].clientX + touches[1].clientX) / 2 - rect.left,
      y: (touches[0].clientY + touches[1].clientY) / 2 - rect.top,
    };
  }

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 2 || !viewportEl) return;
    pinching = true;
    dragging = false;
    suppressClick = true;
    pinchStartDistance = touchDistance(event.touches);
    pinchStartZoom = zoom;
  }

  function onTouchMove(event: TouchEvent) {
    if (!pinching || event.touches.length !== 2 || !viewportEl || pinchStartDistance <= 0) return;
    event.preventDefault();
    const rect = viewportEl.getBoundingClientRect();
    const center = touchCenter(event.touches, rect);
    const scale = touchDistance(event.touches) / pinchStartDistance;
    applyZoomAt(center.x, center.y, pinchStartZoom * scale);
  }

  function onTouchEnd(event: TouchEvent) {
    if (event.touches.length >= 2) return;
    pinching = false;
    pinchStartDistance = 0;
    if (event.touches.length === 0) {
      queueMicrotask(() => {
        suppressClick = false;
      });
    }
  }

  function handleClearClick(event: MouseEvent) {
    event.stopPropagation();
    event.preventDefault();
    clearSelection(true);
  }

  function handleNodeClick(node: ContextMapNode, event: Event) {
    event.stopPropagation();
    if (suppressClick) return;

    const now = Date.now();
    if (lastClick.id === node.id && now - lastClick.time < 360) {
      if (node.kind === "session") {
        onToggleExpandSession?.(node.sessionId);
        queueMicrotask(() => flyToNeighborhood(node.id));
      }
      lastClick = { id: "", time: 0 };
      return;
    }
    lastClick = { id: node.id, time: now };

    onFocusNode?.(node);
    flyToNeighborhood(node.id);
  }

  function handleNodePointerDown(event: PointerEvent) {
    event.stopPropagation();
  }

  function glowRadius(node: ContextMapNode, selected: boolean, hovered: boolean): number {
    const pad = selected ? 6 : hovered ? 4 : node.kind === "session" ? 3 : 2;
    return node.radius + pad;
  }

  function labelMode(
    node: ContextMapNode,
    selected: boolean,
    hovered: boolean,
  ) {
    return resolveMapLabelMode({
      selected,
      hovered,
      ghost: node.renderMode === "ghost",
      inNeighborhood: neighborhood.has(node.id),
      focusActive,
      kind: node.kind,
    });
  }

  function edgeKindClass(kind: string): string {
    if (kind === "membership") return "context-map-edge-membership";
    if (kind === "sequence") return "context-map-edge-sequence";
    return "context-map-edge-session_chain";
  }

  function labelKindClass(kind: ContextMapNode["kind"]): string {
    if (kind === "session") return "context-map-node-label-session";
    if (kind === "thread") return "context-map-node-label-thread";
    if (kind === "claim") return "context-map-node-label-claim";
    return "context-map-node-label-note";
  }

  function labelClass(
    node: ContextMapNode,
    selected: boolean,
    hovered: boolean,
    mode: ReturnType<typeof labelMode>,
  ): string {
    const parts = ["context-map-node-label", labelKindClass(node.kind)];
    if (selected) parts.push("context-map-node-label-selected");
    else if (hovered) parts.push("context-map-node-label-hovered");
    else if (mode === "neighbor") parts.push("context-map-node-label-neighbor");
    else if (mode === "whisper") parts.push("context-map-node-label-whisper");
    if (mode === "hidden") parts.push("context-map-node-label-hidden");
    return parts.join(" ");
  }

  function nodeKindClass(kind: ContextMapNode["kind"]): string {
    if (kind === "session") return "context-map-node-kind-session";
    if (kind === "thread") return "context-map-node-kind-thread";
    if (kind === "claim") return "context-map-node-kind-claim";
    return "context-map-node-kind-note";
  }

  function dotKindClass(kind: ContextMapNode["kind"]): string {
    if (kind === "session") return "context-map-node-dot-session";
    if (kind === "thread") return "context-map-node-dot-thread";
    if (kind === "claim") return "context-map-node-dot-claim";
    return "context-map-node-dot-note";
  }

  function glowKindClass(kind: ContextMapNode["kind"]): string {
    if (kind === "session") return "context-map-node-glow-session";
    if (kind === "thread") return "context-map-node-glow-thread";
    if (kind === "claim") return "context-map-node-glow-claim";
    return "context-map-node-glow-note";
  }

  function legendSwatchClass(kind: ContextMapNode["kind"]): string {
    if (kind === "session") return "context-map-legend-swatch-session";
    if (kind === "thread") return "context-map-legend-swatch-thread";
    if (kind === "claim") return "context-map-legend-swatch-claim";
    return "context-map-legend-swatch-note";
  }

  function nodeClass(node: ContextMapNode, selected: boolean, hovered: boolean): string {
    const parts = ["context-map-node", nodeKindClass(node.kind)];
    if (node.renderMode === "ghost") parts.push("context-map-node-ghost");
    if (selected) parts.push("context-map-node-selected");
    if (hovered) parts.push("context-map-node-hovered");
    if (focusActive && neighborhood.has(node.id)) parts.push("context-map-node-neighbor");
    if (focusActive && !neighborhood.has(node.id)) parts.push("context-map-node-dimmed");
    return parts.join(" ");
  }

  function edgeClass(edge: { from: string; to: string; renderMode?: string; kind: string }): string {
    const parts = ["context-map-edge", edgeKindClass(edge.kind)];
    if (edge.renderMode === "ghost") parts.push("context-map-edge-ghost");
    if (focusActive) {
      if (neighborhood.has(edge.from) && neighborhood.has(edge.to)) {
        parts.push("context-map-edge-neighbor");
      } else {
        parts.push("context-map-edge-dimmed");
      }
    }
    return parts.join(" ");
  }

  function nodeStyle(node: ContextMapNode): string {
    return mapNodeStyleVars(node.kind, node.hue);
  }

  function dotClass(node: ContextMapNode, selected: boolean, hovered: boolean): string {
    const parts = ["context-map-node-dot", dotKindClass(node.kind)];
    if (selected) parts.push("context-map-node-dot-selected");
    else if (hovered) parts.push("context-map-node-dot-hover");
    else if (node.expanded && node.kind === "session") parts.push("context-map-node-dot-expanded");
    return parts.join(" ");
  }
</script>

<div
  bind:this={viewportEl}
  class="context-map-viewport {dragging ? 'context-map-viewport-dragging' : ''} {pinching
    ? 'context-map-viewport-pinching'
    : ''} {focusActive
    ? 'context-map-viewport-focused'
    : ''} {selectionActive ? 'context-map-viewport-selected' : ''}"
  role="application"
  aria-label="Context map canvas"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onwheel={onWheel}
  ontouchstart={onTouchStart}
  ontouchmove={onTouchMove}
  ontouchend={onTouchEnd}
  ontouchcancel={onTouchEnd}
>
  <div class="context-map-controls" role="toolbar" tabindex="-1" onpointerdown={(event) => event.stopPropagation()}>
    {#if selectionActive}
      <button
        type="button"
        class="context-map-control-btn context-map-control-clear"
        aria-label="Clear selection"
        onclick={handleClearClick}
      >
        ✕
      </button>
    {/if}
    <button type="button" class="context-map-control-btn" aria-label="Zoom in" onclick={() => zoomBy(1.18)}>
      +
    </button>
    <button type="button" class="context-map-control-btn" aria-label="Zoom out" onclick={() => zoomBy(1 / 1.18)}>
      −
    </button>
    <button type="button" class="context-map-control-btn" aria-label="Fit graph" onclick={() => fitToView(true)}>
      ◎
    </button>
  </div>

  {#if hoverPreview}
    <div
      class="context-map-hover-card"
      role="region"
      aria-live="polite"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <p class="context-map-hover-kind">{hoverPreview.kind === "session" ? "Session" : "Moment"}</p>
      <p class="context-map-hover-title">{hoverPreview.label}</p>
      <p class="context-map-hover-meta">{neighborSummary(graph, hoverPreview.id)}</p>
      {#if hoverPreview.kind === "session"}
        <p class="context-map-hover-hint">Double-click to expand all moments</p>
      {/if}
    </div>
  {:else if selectionActive && selectedPreview}
    <div
      class="context-map-hover-card context-map-hover-card-pinned"
      role="region"
      aria-live="polite"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <p class="context-map-hover-kind">{selectedPreview.kind === "session" ? "Session" : "Moment"} · focused</p>
      <p class="context-map-hover-title">{selectedPreview.label}</p>
      <button type="button" class="context-map-clear-link" onclick={handleClearClick}>
        Clear focus
      </button>
    </div>
  {/if}

  <div
    class="context-map-legend"
    role="region"
    aria-label="Map legend"
    onpointerdown={(event) => event.stopPropagation()}
  >
    {#each MAP_KIND_LEGEND as entry (entry.kind)}
      <span
        class="context-map-legend-item {entry.planned ? 'context-map-legend-item-planned' : ''}"
        title={entry.planned ? "Coming soon" : entry.label}
      >
        <span class="context-map-legend-swatch {legendSwatchClass(entry.kind)}"></span>
        {entry.shortLabel}
      </span>
    {/each}
  </div>

  <svg class="context-map-svg" width="100%" height="100%" aria-label="Context link map">
    <rect width="100%" height="100%" class="context-map-backdrop" />

    <g transform="translate({panX},{panY}) scale({zoom})" pointer-events="visiblePainted">
      {#each visibleEdges as edge (edge.id)}
        {@const from = nodeById[edge.from]}
        {@const to = nodeById[edge.to]}
        {#if from && to}
          <line
            x1={from.x}
            y1={from.y}
            x2={to.x}
            y2={to.y}
            class={edgeClass(edge)}
          />
        {/if}
      {/each}

      {#each visibleNodes as node (node.id)}
        {@const selected = selectedNodeId === node.id}
        {@const hovered = hoveredNodeId === node.id}
        {@const mode = labelMode(node, selected, hovered)}
        {@const labelText = mapDisplayLabel(node.label, mode, node.kind)}
        <g
          data-map-node
          data-accent={node.kind === "session" ? node.hue % 8 : undefined}
          class={nodeClass(node, selected, hovered)}
          style={node.kind === "session" ? nodeStyle(node) : undefined}
          role="button"
          tabindex="0"
          aria-label="{node.kind === 'session' ? 'Session' : 'Moment'}: {node.label}"
          onclick={(event) => handleNodeClick(node, event)}
          onpointerdown={handleNodePointerDown}
          onmouseenter={() => {
            hoveredNodeId = node.id;
          }}
          onmouseleave={() => {
            if (hoveredNodeId === node.id) hoveredNodeId = null;
          }}
          onkeydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              handleNodeClick(node, event);
            }
          }}
        >
          <circle
            cx={node.x}
            cy={node.y}
            r={glowRadius(node, selected, hovered)}
            class="context-map-node-glow {glowKindClass(node.kind)} {selected
              ? 'context-map-node-glow-selected'
              : hovered
                ? 'context-map-node-glow-hover'
                : node.expanded && node.kind === 'session'
                  ? 'context-map-node-glow-expanded'
                  : ''}"
          />
          {#if node.kind === "session"}
            <circle
              cx={node.x}
              cy={node.y}
              r={node.radius}
              class={dotClass(node, selected, hovered)}
            />
          {:else}
            <rect
              x={node.x - node.radius * 0.82}
              y={node.y - node.radius * 0.82}
              width={node.radius * 1.64}
              height={node.radius * 1.64}
              rx={node.radius * 0.38}
              class={dotClass(node, selected, hovered)}
            />
          {/if}
          {#if mode !== "hidden"}
            <text
              x={node.x}
              y={node.y + node.radius + 14}
              text-anchor="middle"
              class={labelClass(node, selected, hovered, mode)}
            >
              {labelText}
            </text>
          {/if}
        </g>
      {/each}
    </g>
  </svg>
</div>
