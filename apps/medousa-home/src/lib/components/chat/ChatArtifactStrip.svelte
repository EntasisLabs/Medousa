<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import ArtifactFullscreen from "$lib/components/chat/ArtifactFullscreen.svelte";
  import ArtifactPanel from "$lib/components/chat/ArtifactPanel.svelte";
  import type { UiArtifact } from "$lib/types/chat";

  interface Props {
    sessionId: string;
    artifacts: UiArtifact[];
    compact?: boolean;
  }

  let { sessionId, artifacts, compact = false }: Props = $props();

  let panelArtifact = $state<UiArtifact | null>(null);
  let fullscreenArtifact = $state<UiArtifact | null>(null);

  function openPanel(artifact: UiArtifact) {
    panelArtifact = artifact;
  }

  function openFullscreen(artifact: UiArtifact) {
    fullscreenArtifact = artifact;
  }
</script>

{#if artifacts.length > 0}
  <div class="chat-artifact-strip {compact ? 'chat-artifact-strip-compact' : ''}">
    {#each artifacts as artifact (artifact.artifactId)}
      {#if artifact.presentation === "inline"}
        <section class="chat-artifact-card">
          <header class="chat-artifact-card-header">
            <span class="chat-artifact-card-title">{artifact.label}</span>
            <div class="chat-artifact-card-actions">
              <button
                type="button"
                class="chat-artifact-action"
                onclick={() => openPanel(artifact)}
              >
                Panel
              </button>
              <button
                type="button"
                class="chat-artifact-action"
                onclick={() => openFullscreen(artifact)}
              >
                Expand
              </button>
            </div>
          </header>
          <ArtifactEmbed
            {sessionId}
            artifactId={artifact.artifactId}
            label={artifact.label}
            mime={artifact.mime}
            heightPx={artifact.heightPx}
            {compact}
          />
        </section>
      {:else}
        <button
          type="button"
          class="chat-artifact-chip"
          onclick={() =>
            artifact.presentation === "fullscreen"
              ? openFullscreen(artifact)
              : openPanel(artifact)}
        >
          <span class="chat-artifact-chip-label">{artifact.label}</span>
          <span class="chat-artifact-chip-meta">{artifact.presentation}</span>
        </button>
      {/if}
    {/each}
  </div>
{/if}

{#if panelArtifact}
  <ArtifactPanel
    open={true}
    {sessionId}
    artifact={panelArtifact}
    onClose={() => {
      panelArtifact = null;
    }}
  />
{/if}

{#if fullscreenArtifact}
  <ArtifactFullscreen
    open={true}
    {sessionId}
    artifact={fullscreenArtifact}
    onClose={() => {
      fullscreenArtifact = null;
    }}
  />
{/if}

<style>
  .chat-artifact-strip {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-top: 0.75rem;
  }

  .chat-artifact-strip-compact {
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .chat-artifact-card {
    overflow: hidden;
    border-radius: 0.875rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-400) 18%, var(--color-surface-700));
    background: color-mix(in srgb, var(--color-surface-900) 92%, transparent);
  }

  .chat-artifact-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .chat-artifact-card-title {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-surface-200);
  }

  .chat-artifact-card-actions {
    display: flex;
    gap: 0.375rem;
  }

  .chat-artifact-action,
  .chat-artifact-chip {
    border: 0;
    border-radius: 0.5rem;
    padding: 0.25rem 0.5rem;
    font-size: 0.6875rem;
    color: var(--color-primary-200);
    background: color-mix(in srgb, var(--color-primary-500) 12%, var(--color-surface-800));
    cursor: pointer;
  }

  .chat-artifact-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    width: fit-content;
    padding: 0.5rem 0.75rem;
  }

  .chat-artifact-chip-label {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-surface-100);
  }

  .chat-artifact-chip-meta {
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-surface-500);
  }
</style>
