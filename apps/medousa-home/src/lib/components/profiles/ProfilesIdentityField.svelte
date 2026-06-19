<script lang="ts">
  import type { IdentityFieldBlob, IdentityFieldLayout } from "$lib/types/identityField";
  import { fieldViewportForBlobs } from "$lib/utils/identityField";
  import { onMount } from "svelte";

  interface Props {
    layout: IdentityFieldLayout;
    selectedId: string | null;
    loading?: boolean;
    onSelect: (blob: IdentityFieldBlob | null) => void;
  }

  let { layout, selectedId, loading = false, onSelect }: Props = $props();

  let viewportEl: HTMLDivElement | undefined = $state();
  let panX = $state(0);
  let panY = $state(0);
  let scale = $state(1);
  let dragging = $state(false);
  let suppressClick = $state(false);
  let dragOrigin = { x: 0, y: 0, panX: 0, panY: 0 };
  let tick = $state(0);
  let animFrame = 0;

  const driftById = $derived.by(() => {
    tick;
    const t = tick * 0.001;
    const map = new Map<string, { dx: number; dy: number }>();
    layout.blobs.forEach((blob, index) => {
      const amp = blob.kind === "preference" ? 6 : blob.kind === "person" ? 3 : 1.5;
      const phase = index * 1.7;
      map.set(blob.id, {
        dx: Math.sin(t + phase) * amp,
        dy: Math.cos(t * 0.85 + phase) * amp * 0.7,
      });
    });
    return map;
  });

  function fitView() {
    if (!viewportEl) return;
    const { clientWidth, clientHeight } = viewportEl;
    const fit = fieldViewportForBlobs(layout, clientWidth, clientHeight);
    scale = fit.scale;
    panX = fit.offsetX;
    panY = fit.offsetY;
  }

  $effect(() => {
    layout;
    fitView();
  });

  onMount(() => {
    fitView();
    const loop = () => {
      tick = performance.now();
      animFrame = requestAnimationFrame(loop);
    };
    animFrame = requestAnimationFrame(loop);
    const onResize = () => fitView();
    window.addEventListener("resize", onResize);
    return () => {
      cancelAnimationFrame(animFrame);
      window.removeEventListener("resize", onResize);
    };
  });

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
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
    dragging = false;
    viewportEl?.releasePointerCapture(event.pointerId);
  }

  function clientToField(clientX: number, clientY: number): { x: number; y: number } | null {
    if (!viewportEl) return null;
    const rect = viewportEl.getBoundingClientRect();
    const x = (clientX - rect.left - panX) / scale;
    const y = (clientY - rect.top - panY) / scale;
    return { x, y };
  }

  function hitTest(clientX: number, clientY: number): IdentityFieldBlob | null {
    const point = clientToField(clientX, clientY);
    if (!point) return null;
    let hit: IdentityFieldBlob | null = null;
    let best = Infinity;
    for (const blob of layout.blobs) {
      const d = driftById.get(blob.id) ?? { dx: 0, dy: 0 };
      const dx = point.x - (blob.x + d.dx);
      const dy = point.y - (blob.y + d.dy);
      const dist = Math.hypot(dx, dy);
      const hitRadius = blob.radius * (blob.kind === "preference" ? 0.85 : 1);
      if (dist <= hitRadius && dist < best) {
        best = dist;
        hit = blob;
      }
    }
    return hit;
  }

  function onCanvasClick(event: MouseEvent) {
    if (suppressClick) return;
    const hit = hitTest(event.clientX, event.clientY);
    if (hit) {
      onSelect(selectedId === hit.id ? null : hit);
    } else {
      onSelect(null);
    }
  }
</script>

<div class="profiles-field-view">
  <p class="profiles-field-whisper">Drag the field · tap a body to focus</p>

  <div
    bind:this={viewportEl}
    class="profiles-field-viewport {dragging ? 'profiles-field-viewport-dragging' : ''}"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onclick={onCanvasClick}
    role="presentation"
  >
    {#if loading}
      <p class="profiles-field-loading">Gathering who she knows you as…</p>
    {:else}
      <svg
        class="profiles-field-svg"
        width={layout.width}
        height={layout.height}
        viewBox="0 0 {layout.width} {layout.height}"
        style="transform: translate({panX}px, {panY}px) scale({scale}); transform-origin: 0 0;"
        aria-hidden="true"
      >
        <defs>
          <filter id="profiles-field-goo-heavy" x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur in="SourceGraphic" stdDeviation="14" result="blur" />
            <feColorMatrix
              in="blur"
              mode="matrix"
              values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 22 -10"
              result="goo"
            />
            <feBlend in="SourceGraphic" in2="goo" />
          </filter>
          <filter id="profiles-field-goo-soft" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur in="SourceGraphic" stdDeviation="22" result="blur" />
            <feColorMatrix
              in="blur"
              mode="matrix"
              values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 18 -8"
              result="goo"
            />
            <feBlend in="SourceGraphic" in2="goo" />
          </filter>
          <radialGradient id="profiles-cluster-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stop-color="rgb(var(--color-primary-300))" stop-opacity="0.35" />
            <stop offset="100%" stop-color="rgb(var(--color-primary-600))" stop-opacity="0" />
          </radialGradient>
        </defs>

        <!-- preference atmosphere -->
        <g filter="url(#profiles-field-goo-soft)">
          {#each layout.blobs.filter((b) => b.kind === "preference") as blob (blob.id)}
            {@const d = driftById.get(blob.id) ?? { dx: 0, dy: 0 }}
            <circle
              cx={blob.x + d.dx}
              cy={blob.y + d.dy}
              r={blob.radius}
              fill={blob.fill}
              opacity={blob.opacity}
            />
          {/each}
        </g>

        <!-- cluster + people relational mass -->
        <g filter="url(#profiles-field-goo-heavy)">
          {#each layout.blobs.filter((b) => b.kind !== "preference") as blob (blob.id)}
            {@const d = driftById.get(blob.id) ?? { dx: 0, dy: 0 }}
            <circle
              cx={blob.x + d.dx}
              cy={blob.y + d.dy}
              r={blob.radius}
              fill={blob.fill}
              opacity={blob.opacity}
              class={selectedId === blob.id ? "profiles-field-blob-selected" : ""}
            />
          {/each}
        </g>

        <circle
          cx={layout.centerX}
          cy={layout.centerY}
          r={layout.blobs[0]?.radius ?? 90}
          fill="url(#profiles-cluster-glow)"
          pointer-events="none"
        />
      </svg>

      <!-- labels (no goo filter) -->
      <div
        class="profiles-field-labels"
        style="width:{layout.width}px; height:{layout.height}px; transform: translate({panX}px, {panY}px) scale({scale}); transform-origin: 0 0;"
      >
        {#each layout.blobs as blob (blob.id)}
          {@const d = driftById.get(blob.id) ?? { dx: 0, dy: 0 }}
          {@const x = blob.x + d.dx}
          {@const y = blob.y + d.dy}
          {#if blob.kind === "cluster" && !blob.entry}
            <div
              class="profiles-field-label profiles-field-label-center"
              style="left:{x}px; top:{y}px;"
            >
              <span class="profiles-field-label-name">{blob.label}</span>
            </div>
          {:else if blob.kind === "person"}
            <button
              type="button"
              class="profiles-field-label profiles-field-label-person {selectedId === blob.id
                ? 'profiles-field-label-active'
                : ''}"
              style="left:{x}px; top:{y}px;"
              onclick={(event) => {
                event.stopPropagation();
                onSelect(selectedId === blob.id ? null : blob);
              }}
            >
              {blob.label}
            </button>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>
