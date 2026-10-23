<script lang="ts">
  import type { SelectionFrame } from "$lib/create/creativeSelection";
  import { mapPoint } from "$lib/create/creativeScene";
  import type { DrawCamera } from "$lib/draw/drawCamera";
  let { frame, camera }: { frame: SelectionFrame; camera: DrawCamera } = $props();
  const position = $derived(mapPoint(frame.matrix, { x: frame.width / 2, y: -28 / camera.zoom }));
</script>

<!-- Keep transient text out of the artwork SVG: WebKit leaves stale paint over raster images. -->
<span class="create-rotation-label" data-editor-only aria-label="Rotation angle"
  style:left={`${position.x * camera.zoom + camera.panX + 12}px`}
  style:top={`${position.y * camera.zoom + camera.panY}px`}>{Math.round(frame.angle)}°</span>

<style>
  .create-rotation-label {
    position: absolute;
    transform: translateY(-50%);
    z-index: 2;
    pointer-events: none;
    white-space: nowrap;
    padding: 3px 6px;
    border: 1px solid var(--create-line);
    border-radius: 5px;
    background: var(--create-raised);
    color: var(--create-text);
    font-size: 11px;
    line-height: 1.3;
  }
</style>
