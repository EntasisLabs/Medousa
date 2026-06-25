<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import type { UiArtifact } from "$lib/types/chat";

  interface Props {
    open: boolean;
    sessionId: string;
    artifact: UiArtifact;
    onClose: () => void;
  }

  let { open, sessionId, artifact, onClose }: Props = $props();
</script>

{#if open}
  <div class="artifact-fullscreen" role="dialog" aria-modal="true" aria-label={artifact.label}>
    <header class="artifact-fullscreen-header">
      <h3 class="artifact-fullscreen-title">{artifact.label}</h3>
      <button type="button" class="artifact-fullscreen-close" onclick={onClose}>Close</button>
    </header>
    <div class="artifact-fullscreen-body">
      <ArtifactEmbed
        {sessionId}
        artifactId={artifact.artifactId}
        label={artifact.label}
        mime={artifact.mime}
        heightPx={artifact.heightPx ?? 900}
      />
    </div>
  </div>
{/if}

<style>
  .artifact-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    flex-direction: column;
    background: var(--color-surface-950);
  }

  .artifact-fullscreen-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.875rem 1rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 60%, transparent);
  }

  .artifact-fullscreen-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: var(--color-surface-100);
  }

  .artifact-fullscreen-close {
    border: 0;
    border-radius: 0.5rem;
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    color: var(--color-surface-300);
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .artifact-fullscreen-body {
    flex: 1;
    overflow: auto;
    padding: 1rem;
  }
</style>
