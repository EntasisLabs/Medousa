<script lang="ts">
  import type { Snippet } from "svelte";
  import { Maximize2, X } from "@lucide/svelte";

  interface Props {
    title: string;
    onClose: () => void;
    onExpand?: () => void;
    leadingClose?: boolean;
    children: Snippet;
  }

  let { title, onClose, onExpand, leadingClose = false, children }: Props = $props();
</script>

<div class="artifact-chrome">
  <header class="artifact-chrome-bar" class:artifact-chrome-bar-leading-close={leadingClose}>
    {#if leadingClose}
      <button
        type="button"
        class="artifact-chrome-btn artifact-chrome-btn-close-leading"
        aria-label="Close"
        onclick={onClose}
      >
        <X size={16} strokeWidth={2} aria-hidden="true" />
        <span>Close</span>
      </button>
    {/if}
    <h3 class="artifact-chrome-title">{title}</h3>
    <div class="artifact-chrome-actions">
      {#if onExpand}
        <button
          type="button"
          class="artifact-chrome-btn artifact-chrome-btn-secondary"
          aria-label="Expand fullscreen"
          onclick={onExpand}
        >
          <Maximize2 size={14} strokeWidth={2} aria-hidden="true" />
          <span>Expand</span>
        </button>
      {/if}
      {#if !leadingClose}
        <button
          type="button"
          class="artifact-chrome-btn"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={14} strokeWidth={2} aria-hidden="true" />
          <span>Close</span>
        </button>
      {/if}
    </div>
  </header>
  <div class="artifact-chrome-body">
    {@render children()}
  </div>
</div>
