<script lang="ts">
  import Eraser from "@lucide/svelte/icons/eraser";
  import Hand from "@lucide/svelte/icons/hand";
  import MousePointer2 from "@lucide/svelte/icons/mouse-pointer-2";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Redo2 from "@lucide/svelte/icons/redo-2";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import {
    mobileDrawControls,
    type MobileDrawTool,
  } from "$lib/draw/mobileDrawControls.svelte";
  import { onDestroy, untrack } from "svelte";

  interface Props {
    tool: MobileDrawTool;
    optionsOpen: boolean;
    canUndo: boolean;
    canRedo: boolean;
    onTool: (tool: MobileDrawTool) => void;
    onOptions: () => void;
    onUndo: () => void;
    onRedo: () => void;
    mobileVisible?: boolean;
  }

  let { tool, optionsOpen, canUndo, canRedo, onTool, onOptions, onUndo, onRedo, mobileVisible = false }: Props = $props();
  const owner = {};

  $effect(() => {
    const controls = {
      tool,
      optionsOpen,
      canUndo,
      canRedo,
      setTool: onTool,
      openOptions: onOptions,
      undo: onUndo,
      redo: onRedo,
    };
    untrack(() => mobileDrawControls.register(owner, controls));
  });

  onDestroy(() => mobileDrawControls.unregister(owner));

  export function activateMobileControls() {
    mobileDrawControls.activate(owner);
  }
</script>

<div class="medousa-draw-toolbar" class:mobile-visible={mobileVisible} role="toolbar" aria-label="Drawing tools">
  <div class="medousa-draw-tools" role="group" aria-label="Active tool">
    <button type="button" class:active={tool === "ink"} aria-pressed={tool === "ink"} onclick={() => onTool("ink")}>
      <PenLine size={18} strokeWidth={2} aria-hidden="true" />
      <span>Draw</span>
    </button>
    <button type="button" class:active={tool === "eraser"} aria-pressed={tool === "eraser"} onclick={() => onTool("eraser")}>
      <Eraser size={18} strokeWidth={2} aria-hidden="true" />
      <span>Erase</span>
    </button>
    <button type="button" class:active={tool === "select"} aria-pressed={tool === "select"} onclick={() => onTool("select")}>
      <MousePointer2 size={18} strokeWidth={2} aria-hidden="true" />
      <span>Select</span>
    </button>
    <button type="button" class:active={tool === "hand"} aria-pressed={tool === "hand"} onclick={() => onTool("hand")}>
      <Hand size={18} strokeWidth={2} aria-hidden="true" />
      <span>Move</span>
    </button>
  </div>
  <div class="medousa-draw-quick-actions" role="group" aria-label="Drawing actions">
    <button
      type="button"
      class:active={optionsOpen}
      aria-label="Drawing options"
      aria-expanded={optionsOpen}
      onclick={onOptions}
    >
      <SlidersHorizontal size={19} strokeWidth={2} aria-hidden="true" />
    </button>
    <button type="button" aria-label="Undo" disabled={!canUndo} onclick={onUndo}>
      <Undo2 size={19} strokeWidth={2} aria-hidden="true" />
    </button>
    <button type="button" aria-label="Redo" disabled={!canRedo} onclick={onRedo}>
      <Redo2 size={19} strokeWidth={2} aria-hidden="true" />
    </button>
  </div>
</div>

<style>
  .medousa-draw-toolbar {
    z-index: 10;
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: 3.5rem;
    padding: 0.4rem max(0.45rem, env(safe-area-inset-left, 0px));
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.22);
    background: rgb(var(--color-surface-900) / 0.94);
    backdrop-filter: blur(18px);
  }

  .medousa-draw-tools,
  .medousa-draw-quick-actions { display: flex; align-items: center; gap: 0.2rem; }
  .medousa-draw-tools { min-width: 0; }

  button {
    display: inline-flex;
    min-width: 2.75rem;
    height: 2.65rem;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0 0.65rem;
    border-radius: 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
    font-weight: 600;
    white-space: nowrap;
  }

  button:hover:not(:disabled),
  button.active { color: rgb(var(--color-surface-50)); background: rgb(var(--color-surface-500) / 0.28); }
  button.active { box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400) / 0.42); }
  button:disabled { opacity: 0.3; }

  .medousa-draw-quick-actions {
    flex: 0 0 auto;
    padding-left: 0.35rem;
    border-left: 1px solid rgb(var(--color-surface-500) / 0.2);
  }

  :global(.mobile-shell) .medousa-draw-toolbar:not(.mobile-visible) { display: none; }

  @container (max-width: 640px) {
    .medousa-draw-toolbar {
      gap: 0.25rem;
      padding-right: max(0.35rem, env(safe-area-inset-right, 0px));
      padding-left: max(0.35rem, env(safe-area-inset-left, 0px));
    }
    .medousa-draw-tools { flex: 1 1 auto; justify-content: space-between; }
    .medousa-draw-tools button { min-width: 2.55rem; padding: 0 0.5rem; }
    .medousa-draw-tools button span { display: none; }
    .medousa-draw-quick-actions { gap: 0; padding-left: 0.2rem; }
    .medousa-draw-quick-actions button { min-width: 2.5rem; padding: 0; }
  }
</style>
