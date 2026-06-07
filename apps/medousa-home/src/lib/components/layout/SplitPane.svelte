<script lang="ts">
  interface Props {
    width: number;
    min?: number;
    max?: number;
    side?: "left" | "right";
    onResize: (width: number) => void;
    children: import("svelte").Snippet;
  }

  let {
    width,
    min = 180,
    max = 520,
    side = "right",
    onResize,
    children,
  }: Props = $props();

  let dragging = $state(false);

  function clamp(value: number): number {
    return Math.min(max, Math.max(min, value));
  }

  function onPointerDown(event: PointerEvent) {
    event.preventDefault();
    dragging = true;
    const startX = event.clientX;
    const startWidth = width;

    function onMove(moveEvent: PointerEvent) {
      const delta =
        side === "right" ? startX - moveEvent.clientX : moveEvent.clientX - startX;
      onResize(clamp(startWidth + delta));
    }

    function onUp() {
      dragging = false;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }
</script>

<div
  class="relative flex h-full shrink-0 flex-col"
  style="width: {width}px"
>
  {#if side === "left"}
    <button
      type="button"
      aria-label="Resize panel"
      class="absolute right-0 top-0 z-10 h-full w-1.5 cursor-col-resize border-0 bg-transparent hover:bg-primary-500/30 {dragging
        ? 'bg-primary-500/40'
        : ''}"
      onpointerdown={onPointerDown}
    ></button>
  {/if}

  {@render children()}

  {#if side === "right"}
    <button
      type="button"
      aria-label="Resize panel"
      class="absolute left-0 top-0 z-10 h-full w-1.5 cursor-col-resize border-0 bg-transparent hover:bg-primary-500/30 {dragging
        ? 'bg-primary-500/40'
        : ''}"
      onpointerdown={onPointerDown}
    ></button>
  {/if}
</div>
