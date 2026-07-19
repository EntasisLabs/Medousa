<script lang="ts">
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    onHide: () => void;
  }

  let { onHide }: Props = $props();

  const HEIGHT_KEY = "medousa.scripts.consoleHeightPx";
  const MIN_H = 120;
  const MAX_H = 480;
  const DEFAULT_H = 220;

  function loadHeight(): number {
    if (typeof localStorage === "undefined") return DEFAULT_H;
    const raw = Number(localStorage.getItem(HEIGHT_KEY));
    if (!Number.isFinite(raw)) return DEFAULT_H;
    return Math.min(MAX_H, Math.max(MIN_H, raw));
  }

  let heightPx = $state(loadHeight());
  let dragging = $state(false);

  function persistHeight(next: number) {
    heightPx = next;
    try {
      localStorage.setItem(HEIGHT_KEY, String(next));
    } catch {
      // ignore
    }
  }

  function onResizePointerDown(event: PointerEvent) {
    event.preventDefault();
    const startY = event.clientY;
    const startH = heightPx;
    dragging = true;
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);

    const onMove = (move: PointerEvent) => {
      const delta = startY - move.clientY;
      const next = Math.min(MAX_H, Math.max(MIN_H, startH + delta));
      heightPx = next;
    };
    const onUp = (up: PointerEvent) => {
      dragging = false;
      persistHeight(heightPx);
      target.releasePointerCapture(up.pointerId);
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  }
</script>

<div
  class="scripts-workbench-console shrink-0 border-t border-surface-500/40 {dragging
    ? 'is-resizing'
    : ''}"
  style="height: {heightPx}px"
>
  <div
    class="scripts-workbench-console-resizer"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize output panel"
    aria-valuemin={MIN_H}
    aria-valuemax={MAX_H}
    aria-valuenow={Math.round(heightPx)}
    tabindex="0"
    onpointerdown={onResizePointerDown}
    onkeydown={(event) => {
      if (event.key === "ArrowUp") {
        event.preventDefault();
        persistHeight(Math.min(MAX_H, heightPx + 16));
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        persistHeight(Math.max(MIN_H, heightPx - 16));
      }
    }}
  ></div>
  <div class="scripts-workbench-console-head">
    <p class="text-[10px] font-medium tracking-[-0.01em] text-surface-400">Output</p>
    <button
      type="button"
      class="text-[10px] text-surface-500 transition hover:text-surface-300"
      onclick={onHide}
    >
      Hide
    </button>
  </div>
  <div class="scripts-workbench-console-body">
    {#if graphemeScriptEditor.compileError}
      <p class="mb-2 font-mono text-[11px] text-error-400">
        {graphemeScriptEditor.compileError}
      </p>
    {:else if graphemeScriptEditor.compileResult}
      <div class="mb-2 space-y-1 font-mono text-[11px] text-surface-400">
        {#each graphemeScriptEditor.compileResult.compile_hints as hint (hint)}
          <p>{hint}</p>
        {/each}
        {#each graphemeScriptEditor.compileResult.lint_warnings as warning (warning)}
          <p class="text-warning-400">{warning}</p>
        {/each}
      </div>
    {/if}
    <GraphemeRunResultCard
      result={workshop.runResult?.result}
      error={workshop.runError}
      emptyMessage="Run or compile to see output here."
    />
  </div>
</div>
