<script lang="ts">
  import type { Snippet } from "svelte";
  import { Maximize2, X } from "@lucide/svelte";
  import ArtifactExportMenu from "$lib/components/chat/ArtifactExportMenu.svelte";

  interface Props {
    title: string;
    onClose: () => void;
    onExpand?: () => void;
    leadingClose?: boolean;
    sessionId?: string;
    artifactId?: string;
    children: Snippet;
  }

  let {
    title,
    onClose,
    onExpand,
    leadingClose = false,
    sessionId,
    artifactId,
    children,
  }: Props = $props();

  let exportStatus = $state<string | null>(null);
  let exportStatusTimer: ReturnType<typeof setTimeout> | undefined;

  function handleExportStatus(message: string | null) {
    if (exportStatusTimer) clearTimeout(exportStatusTimer);
    exportStatus = message;
    if (message) {
      exportStatusTimer = setTimeout(() => {
        exportStatus = null;
      }, 3200);
    }
  }

  const showExport = $derived(Boolean(sessionId?.trim() && artifactId?.trim()));
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
    <div class="artifact-chrome-title-wrap">
      <h3 class="artifact-chrome-title">{title}</h3>
      {#if exportStatus}
        <p class="artifact-chrome-status" aria-live="polite">{exportStatus}</p>
      {/if}
    </div>
    <div class="artifact-chrome-actions">
      {#if showExport}
        <ArtifactExportMenu
          sessionId={sessionId!}
          artifactId={artifactId!}
          label={title}
          onStatus={handleExportStatus}
        />
      {/if}
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
