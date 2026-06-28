<script lang="ts">
  import { onMount } from "svelte";
  import {
    Copy,
    FileDown,
    FileImage,
    FileText,
    MoreHorizontal,
    Share2,
  } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import {
    copyArtifactId,
    copyArtifactPath,
    exportArtifactHtml,
    exportArtifactPdf,
    exportArtifactPng,
    shareArtifact,
  } from "$lib/utils/artifactExport";

  interface Props {
    sessionId: string;
    artifactId: string;
    label: string;
    compact?: boolean;
    onStatus?: (message: string | null) => void;
  }

  let {
    sessionId,
    artifactId,
    label,
    compact = false,
    onStatus,
  }: Props = $props();

  let open = $state(false);
  let busy = $state(false);
  let menuEl = $state<HTMLDivElement | null>(null);

  function setStatus(message: string | null) {
    onStatus?.(message);
  }

  function closeMenu() {
    open = false;
  }

  onMount(() => {
    function handlePointerDown(event: MouseEvent) {
      if (!open || !menuEl) return;
      if (event.target instanceof Node && menuEl.contains(event.target)) return;
      closeMenu();
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") closeMenu();
    }

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeydown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeydown);
    };
  });

  async function runAction(
    action: () => Promise<boolean | "shared" | "copied" | "failed">,
    successMessage: string,
    cancelMessage = "Cancelled.",
  ) {
    if (busy) return;
    busy = true;
    haptic("light");
    closeMenu();
    try {
      const result = await action();
      if (result === false) {
        setStatus(cancelMessage);
        return;
      }
      if (result === "failed") {
        setStatus("Could not share the presentation.");
        return;
      }
      if (result === "copied") {
        setStatus("Copied to clipboard.");
        return;
      }
      if (result === "shared") {
        setStatus("Shared.");
        return;
      }
      setStatus(successMessage);
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }
</script>

<div class="artifact-export-menu" bind:this={menuEl}>
  <button
    type="button"
    class="artifact-chrome-btn artifact-chrome-btn-secondary"
    class:artifact-export-menu-compact={compact}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="Share and export presentation"
    disabled={busy}
    onclick={() => {
      haptic("light");
      open = !open;
    }}
  >
    <MoreHorizontal size={14} strokeWidth={2} aria-hidden="true" />
    {#if !compact}
      <span>Share</span>
    {/if}
  </button>

  {#if open}
    <div class="artifact-export-dropdown" role="menu">
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(
            () => shareArtifact(sessionId, artifactId, label),
            "Shared.",
          )}
      >
        <Share2 size={14} strokeWidth={2} aria-hidden="true" />
        Share summary
      </button>
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(
            () => copyArtifactPath(sessionId, artifactId),
            "Path copied.",
          )}
      >
        <Copy size={14} strokeWidth={2} aria-hidden="true" />
        Copy path
      </button>
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(() => copyArtifactId(artifactId), "Artifact ID copied.")}
      >
        <Copy size={14} strokeWidth={2} aria-hidden="true" />
        Copy artifact ID
      </button>
      <div class="artifact-export-divider" aria-hidden="true"></div>
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(
            () => exportArtifactPdf(sessionId, artifactId, label),
            "Saved as PDF.",
          )}
      >
        <FileDown size={14} strokeWidth={2} aria-hidden="true" />
        Save as PDF
      </button>
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(
            () => exportArtifactPng(sessionId, artifactId, label),
            "Saved as PNG.",
          )}
      >
        <FileImage size={14} strokeWidth={2} aria-hidden="true" />
        Save as PNG
      </button>
      <button
        type="button"
        class="artifact-export-item"
        role="menuitem"
        onclick={() =>
          void runAction(
            () => exportArtifactHtml(sessionId, artifactId, label),
            "Saved as HTML.",
          )}
      >
        <FileText size={14} strokeWidth={2} aria-hidden="true" />
        Save as HTML
      </button>
    </div>
  {/if}
</div>

<style>
  .artifact-export-menu {
    position: relative;
  }

  .artifact-export-menu-compact {
    padding-inline: 0.45rem;
  }

  .artifact-export-dropdown {
    position: absolute;
    top: calc(100% + 0.35rem);
    right: 0;
    z-index: 140;
    min-width: 11.5rem;
    overflow: hidden;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 55%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.28);
    padding: 0.35rem;
  }

  :global(html:not(.dark)) .artifact-export-dropdown {
    background: rgb(var(--color-surface-50));
    border-color: rgb(var(--color-surface-300) / 0.9);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.08);
  }

  .artifact-export-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.5rem;
    border: 0;
    border-radius: 0.5rem;
    padding: 0.45rem 0.55rem;
    text-align: left;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
    background: transparent;
    cursor: pointer;
  }

  :global(html:not(.dark)) .artifact-export-item {
    color: rgb(var(--color-surface-800));
  }

  .artifact-export-item:hover {
    background: color-mix(in srgb, var(--color-primary-500) 14%, var(--color-surface-800));
  }

  :global(html:not(.dark)) .artifact-export-item:hover {
    background: rgb(var(--color-surface-200));
  }

  .artifact-export-divider {
    height: 1px;
    margin: 0.25rem 0.15rem;
    background: color-mix(in srgb, var(--color-surface-500) 35%, transparent);
  }
</style>
