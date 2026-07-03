<script lang="ts">
  import { onMount } from "svelte";
  import { Code2, MoreHorizontal, Pencil, Trash2 } from "@lucide/svelte";

  interface Props {
    disabled?: boolean;
    onViewSource: () => void;
    onEdit: () => void;
    onDelete: () => void;
  }

  let { disabled = false, onViewSource, onEdit, onDelete }: Props = $props();

  let open = $state(false);
  let menuEl = $state<HTMLDivElement | null>(null);

  function closeMenu() {
    open = false;
  }

  onMount(() => {
    function handlePointerDown(event: MouseEvent) {
      if (!open || !menuEl) return;
      if (!menuEl.contains(event.target as Node)) closeMenu();
    }
    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  });
</script>

<div class="presentation-artifact-menu" bind:this={menuEl}>
  <button
    type="button"
    class="presentation-artifact-menu-trigger"
    aria-label="Presentation actions"
    aria-expanded={open}
    {disabled}
    onclick={() => {
      open = !open;
    }}
  >
    <MoreHorizontal size={16} strokeWidth={2} aria-hidden="true" />
  </button>
  {#if open}
    <menu class="presentation-artifact-menu-panel">
      <button
        type="button"
        class="presentation-artifact-menu-item"
        onclick={() => {
          closeMenu();
          onViewSource();
        }}
      >
        <Code2 size={14} aria-hidden="true" />
        View source
      </button>
      <button
        type="button"
        class="presentation-artifact-menu-item"
        onclick={() => {
          closeMenu();
          onEdit();
        }}
      >
        <Pencil size={14} aria-hidden="true" />
        Edit HTML
      </button>
      <button
        type="button"
        class="presentation-artifact-menu-item presentation-artifact-menu-item-danger"
        onclick={() => {
          closeMenu();
          onDelete();
        }}
      >
        <Trash2 size={14} aria-hidden="true" />
        Delete widget
      </button>
    </menu>
  {/if}
</div>

<style>
  .presentation-artifact-menu {
    position: relative;
  }

  .presentation-artifact-menu-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 45%, transparent);
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-900) 72%, transparent);
    cursor: pointer;
  }

  .presentation-artifact-menu-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .presentation-artifact-menu-panel {
    position: absolute;
    top: calc(100% + 0.35rem);
    right: 0;
    z-index: 6;
    min-width: 10.5rem;
    margin: 0;
    padding: 0.35rem;
    list-style: none;
    border-radius: 0.625rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.24);
  }

  .presentation-artifact-menu-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.5rem;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font-size: 0.75rem;
    text-align: left;
    color: rgb(var(--color-surface-100));
    background: transparent;
    cursor: pointer;
  }

  .presentation-artifact-menu-item:hover {
    background: color-mix(in srgb, var(--color-surface-700) 65%, transparent);
  }

  .presentation-artifact-menu-item-danger {
    color: rgb(var(--color-error-300));
  }
</style>
