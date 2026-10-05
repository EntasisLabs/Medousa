<script lang="ts">
  import { onMount, onDestroy, type Snippet } from "svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  let { workId, name, children }: { workId: string; name: string; children: Snippet } = $props();
  let height = $state(320);
  let expanded = $state(false);
  let returnFocus: HTMLElement | null = null;
  let stopDrag: (() => void) | null = null;
  const key = $derived(`medousa:code-panel:${activeWorkshopId() ?? "local"}:${workId}:${name}`);
  function resize(value: number) {
    height = Math.max(180, Math.min(640, Math.round(value)));
    try { localStorage.setItem(key, String(height)); } catch { /* A panel is still usable without persisted preferences. */ }
  }
  function drag(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault(); stopDrag?.();
    if (expanded) { resize(Math.min(window.innerHeight * 0.65, 640)); expanded = false; }
    const y = event.clientY, start = height;
    const move = (next: PointerEvent) => resize(start + y - next.clientY);
    const stop = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", stop); window.removeEventListener("pointercancel", stop); stopDrag = null; };
    stopDrag = stop;
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", stop); window.addEventListener("pointercancel", stop);
  }
  function keys(event: KeyboardEvent) {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault(); resize(height + (event.key === "ArrowUp" ? 24 : -24));
  }
  $effect(() => {
    void key;
    try { const saved = Number(localStorage.getItem(key)); height = saved >= 180 && saved <= 640 ? saved : 320; } catch { height = 320; }
  });
  onMount(() => { returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null; });
  onDestroy(() => { stopDrag?.(); if (returnFocus?.isConnected) returnFocus.focus(); });
</script>
<section class="flex min-h-0 shrink-0 flex-col border-t border-surface-500/30 bg-surface-950" style:height={expanded ? "65vh" : `${height}px`} style:max-height="70vh" aria-label={`${name} panel`}>
  <div class="flex shrink-0 items-center border-b border-surface-500/15">
    <div class="h-2 min-w-0 flex-1 cursor-row-resize focus:bg-primary-500/25 focus:outline-none" role="slider" tabindex="0" aria-label={`Resize ${name} panel`} aria-orientation="vertical" aria-valuemin="180" aria-valuemax="640" aria-valuenow={height} onpointerdown={drag} onkeydown={keys}></div>
    <button type="button" class="px-2 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800" aria-pressed={expanded} onclick={() => (expanded = !expanded)}>{expanded ? "Restore size" : "Expand"}</button>
  </div>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden">{@render children()}</div>
</section>
