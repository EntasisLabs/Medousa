<script lang="ts">
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  interface Props {
    branchId: string;
    direction: "row" | "column";
    ratio: number;
  }

  let { branchId, direction, ratio }: Props = $props();

  let dragging = $state(false);

  function onPointerDown(event: PointerEvent) {
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    const parent = target.parentElement;
    if (!parent) return;
    dragging = true;
    target.setPointerCapture(event.pointerId);
    const vertical = direction === "column";
    const rect = parent.getBoundingClientRect();

    const onMove = (moveEvent: PointerEvent) => {
      const pos = vertical
        ? (moveEvent.clientX - rect.left) / rect.width
        : (moveEvent.clientY - rect.top) / rect.height;
      shellTabs.setRatio(branchId, pos);
    };
    const onUp = () => {
      dragging = false;
      try {
        target.releasePointerCapture(event.pointerId);
      } catch {
        /* already released (e.g. OS overlay stole focus) */
      }
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("blur", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    // Greenshot / Win+Shift+S steal HWND focus mid-drag — release capture.
    window.addEventListener("blur", onUp);
  }

  function onDblClick() {
    shellTabs.setRatio(branchId, 0.5);
  }
</script>

<div
  class="editor-split-sash {direction === 'column'
    ? 'editor-split-sash-col'
    : 'editor-split-sash-row'} {dragging ? 'editor-split-sash-active' : ''}"
  role="separator"
  aria-orientation={direction === "column" ? "vertical" : "horizontal"}
  aria-valuenow={Math.round(ratio * 100)}
  tabindex="0"
  onpointerdown={onPointerDown}
  ondblclick={onDblClick}
></div>

<style>
  .editor-split-sash {
    flex-shrink: 0;
    z-index: 2;
    background: transparent;
  }
  .editor-split-sash-col {
    width: 4px;
    cursor: col-resize;
    margin: 0 -1px;
  }
  .editor-split-sash-row {
    height: 4px;
    cursor: row-resize;
    margin: -1px 0;
  }
  .editor-split-sash:hover,
  .editor-split-sash-active {
    background: color-mix(in oklab, var(--color-primary-500, #8b7cf8) 45%, transparent);
  }
</style>
