<script lang="ts">
  import CanvasWidgetCatalog from "$lib/components/settings/CanvasWidgetCatalog.svelte";
  import { X } from "@lucide/svelte";
  import { cubicOut } from "svelte/easing";
  import { fade, fly } from "svelte/transition";

  interface Props {
    open: boolean;
    surfaceId: string;
    onClose: () => void;
    onAdded?: (componentId: string) => void;
  }

  let { open, surfaceId, onClose, onAdded }: Props = $props();
</script>

{#if open}
  <div
    class="canvas-widget-modal-backdrop"
    role="presentation"
    transition:fade={{ duration: 180 }}
    onclick={onClose}
  ></div>
  <div
    class="canvas-widget-modal"
    role="dialog"
    aria-modal="true"
    aria-label="Add widget"
    transition:fly={{ y: 20, duration: 240, easing: cubicOut }}
  >
    <header class="canvas-widget-modal-header">
      <h2 class="canvas-widget-modal-title">Add widget</h2>
      <button type="button" class="canvas-widget-modal-close" aria-label="Close" onclick={onClose}>
        <X size={18} />
      </button>
    </header>
    <div class="canvas-widget-modal-body">
      <CanvasWidgetCatalog
        defaultSurfaceId={surfaceId}
        compact={true}
        onAdded={(componentId) => {
          onAdded?.(componentId);
          onClose();
        }}
      />
    </div>
  </div>
{/if}

<style>
  .canvas-widget-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgb(0 0 0 / 0.45);
  }

  .canvas-widget-modal {
    position: fixed;
    z-index: 61;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(28rem, calc(100vw - 1.5rem));
    max-height: min(34rem, calc(100vh - 2rem));
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.4);
  }

  .canvas-widget-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.75rem 0.85rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
  }

  .canvas-widget-modal-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .canvas-widget-modal-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border-radius: 0.45rem;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .canvas-widget-modal-body {
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0.85rem;
    min-width: 0;
  }
</style>
