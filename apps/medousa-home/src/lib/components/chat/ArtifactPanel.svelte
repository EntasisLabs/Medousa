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
  <div class="artifact-panel-backdrop" role="presentation" onclick={onClose}></div>
  <aside class="artifact-panel" aria-label={artifact.label}>
    <header class="artifact-panel-header">
      <h3 class="artifact-panel-title">{artifact.label}</h3>
      <button type="button" class="artifact-panel-close" onclick={onClose}>Close</button>
    </header>
    <div class="artifact-panel-body">
      <ArtifactEmbed
        {sessionId}
        artifactId={artifact.artifactId}
        label={artifact.label}
        mime={artifact.mime}
        heightPx={artifact.heightPx ?? 720}
      />
    </div>
  </aside>
{/if}

<style>
  .artifact-panel-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgb(0 0 0 / 0.45);
  }

  .artifact-panel {
    position: fixed;
    top: 0;
    right: 0;
    z-index: 61;
    display: flex;
    flex-direction: column;
    width: min(42rem, 100vw);
    height: 100vh;
    border-left: 1px solid color-mix(in srgb, var(--color-surface-600) 70%, transparent);
    background: var(--color-surface-950);
    box-shadow: -12px 0 40px rgb(0 0 0 / 0.25);
  }

  .artifact-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.875rem 1rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 60%, transparent);
  }

  .artifact-panel-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--color-surface-100);
  }

  .artifact-panel-close {
    border: 0;
    border-radius: 0.5rem;
    padding: 0.375rem 0.625rem;
    font-size: 0.75rem;
    color: var(--color-surface-300);
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .artifact-panel-body {
    flex: 1;
    overflow: auto;
    padding: 0.75rem;
  }
</style>
