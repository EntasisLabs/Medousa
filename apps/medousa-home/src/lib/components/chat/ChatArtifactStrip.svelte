<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import ArtifactFullscreen from "$lib/components/chat/ArtifactFullscreen.svelte";
  import ArtifactPanel from "$lib/components/chat/ArtifactPanel.svelte";
  import { haptic } from "$lib/haptics";
  import type { UiArtifact } from "$lib/types/chat";
  import { LayoutPanelLeft, Maximize2, Sparkles } from "@lucide/svelte";

  interface Props {
    sessionId: string;
    artifacts: UiArtifact[];
    compact?: boolean;
  }

  let { sessionId, artifacts, compact = false }: Props = $props();

  let panelArtifact = $state<UiArtifact | null>(null);
  let fullscreenArtifact = $state<UiArtifact | null>(null);
  let autoOpened = $state<Set<string>>(new Set());

  $effect(() => {
    for (const artifact of artifacts) {
      if (autoOpened.has(artifact.artifactId)) continue;
      if (artifact.presentation === "panel") {
        panelArtifact = artifact;
        autoOpened = new Set(autoOpened).add(artifact.artifactId);
      } else if (artifact.presentation === "fullscreen") {
        fullscreenArtifact = artifact;
        autoOpened = new Set(autoOpened).add(artifact.artifactId);
      }
    }
  });

  function openPanel(artifact: UiArtifact) {
    haptic("light");
    panelArtifact = artifact;
    fullscreenArtifact = null;
  }

  function openFullscreen(artifact: UiArtifact) {
    haptic("light");
    fullscreenArtifact = artifact;
    panelArtifact = null;
  }

  function closePanel() {
    haptic("light");
    panelArtifact = null;
  }

  function closeFullscreen() {
    haptic("light");
    fullscreenArtifact = null;
  }

  function expandPanelToFullscreen() {
    if (!panelArtifact) return;
    openFullscreen(panelArtifact);
  }
</script>

{#if artifacts.length > 0}
  <div class="chat-artifact-strip {compact ? 'chat-artifact-strip-compact' : ''}">
    {#each artifacts as artifact (artifact.artifactId)}
      {#if artifact.presentation === "inline"}
        <section
          class="chat-artifact-card"
          class:chat-artifact-card-dimmed={panelArtifact?.artifactId === artifact.artifactId ||
            fullscreenArtifact?.artifactId === artifact.artifactId}
        >
          <header class="chat-artifact-card-header">
            <div class="chat-artifact-card-heading">
              <span class="chat-artifact-card-eyebrow">
                <Sparkles size={11} strokeWidth={2} aria-hidden="true" />
                Presentation
              </span>
              <span class="chat-artifact-card-title">{artifact.label}</span>
            </div>
            <div class="chat-artifact-card-actions">
              <button
                type="button"
                class="chat-artifact-action chat-artifact-action-primary"
                aria-label="Open {artifact.label} in side panel"
                onclick={() => openPanel(artifact)}
              >
                <LayoutPanelLeft size={13} strokeWidth={2} aria-hidden="true" />
                Open
              </button>
              <button
                type="button"
                class="chat-artifact-action"
                aria-label="Expand {artifact.label} fullscreen"
                onclick={() => openFullscreen(artifact)}
              >
                <Maximize2 size={13} strokeWidth={2} aria-hidden="true" />
                Expand
              </button>
            </div>
          </header>
          <div class="chat-artifact-card-body">
            <ArtifactEmbed
              {sessionId}
              artifactId={artifact.artifactId}
              label={artifact.label}
              mime={artifact.mime}
              heightPx={artifact.heightPx}
              bare={true}
              mode="inline"
              onOpenFull={() => openPanel(artifact)}
              {compact}
            />
          </div>
        </section>
      {:else}
        <button
          type="button"
          class="chat-artifact-launch"
          onclick={() =>
            artifact.presentation === "fullscreen"
              ? openFullscreen(artifact)
              : openPanel(artifact)}
        >
          <span class="chat-artifact-launch-copy">
            <span class="chat-artifact-launch-eyebrow">
              <Sparkles size={11} strokeWidth={2} aria-hidden="true" />
              Presentation
            </span>
            <span class="chat-artifact-launch-label">{artifact.label}</span>
          </span>
          <span class="chat-artifact-launch-action">Open</span>
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
    onClose={closePanel}
    onExpand={expandPanelToFullscreen}
  />
{/if}

{#if fullscreenArtifact}
  <ArtifactFullscreen
    open={true}
    {sessionId}
    artifact={fullscreenArtifact}
    onClose={closeFullscreen}
  />
{/if}

<style>
  .chat-artifact-strip {
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
    margin-top: 0.875rem;
  }

  .chat-artifact-strip-compact {
    gap: 0.625rem;
    margin-top: 0.625rem;
  }

  .chat-artifact-card {
    overflow: hidden;
    border-radius: 1rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-400) 20%, var(--color-surface-600));
    background: rgb(var(--color-surface-900));
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.14);
    transition:
      opacity 180ms ease,
      transform 180ms ease,
      box-shadow 180ms ease;
  }

  :global(html:not(.dark)) .chat-artifact-card {
    background: rgb(var(--color-surface-50));
    border-color: rgb(var(--color-surface-300) / 0.9);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.06);
  }

  .chat-artifact-card-dimmed {
    opacity: 0.45;
    transform: scale(0.992);
    pointer-events: none;
  }

  .chat-artifact-card-header {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem 0.5rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-600) 35%, transparent);
    background: rgb(var(--color-surface-900));
  }

  :global(html:not(.dark)) .chat-artifact-card-header {
    background: rgb(var(--color-surface-100));
    border-bottom-color: rgb(var(--color-surface-300) / 0.85);
  }

  .chat-artifact-card-heading {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.125rem;
  }

  .chat-artifact-card-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--color-primary-400) 80%, var(--color-surface-500));
  }

  :global(html:not(.dark)) .chat-artifact-card-eyebrow {
    color: color-mix(in srgb, var(--color-primary-600) 75%, var(--color-surface-600));
  }

  .chat-artifact-card-title {
    font-size: 0.8125rem;
    font-weight: 600;
    line-height: 1.3;
    color: rgb(var(--color-surface-100));
  }

  :global(html:not(.dark)) .chat-artifact-card-title {
    color: rgb(var(--color-surface-900));
  }

  .chat-artifact-card-actions {
    display: flex;
    flex-shrink: 0;
    gap: 0.375rem;
  }

  .chat-artifact-action {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 65%, transparent);
    border-radius: 999px;
    padding: 0.32rem 0.625rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 72%, var(--color-surface-900));
    cursor: pointer;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }

  :global(html:not(.dark)) .chat-artifact-action {
    color: rgb(var(--color-surface-800));
    background: rgb(var(--color-surface-50));
    border-color: rgb(var(--color-surface-400) / 0.95);
  }

  .chat-artifact-action:hover {
    color: rgb(var(--color-surface-50));
    border-color: color-mix(in srgb, var(--color-primary-400) 45%, var(--color-surface-500));
    background: color-mix(in srgb, var(--color-primary-500) 18%, var(--color-surface-800));
  }

  :global(html:not(.dark)) .chat-artifact-action:hover {
    color: rgb(var(--color-surface-900));
    background: rgb(var(--color-surface-200));
  }

  .chat-artifact-action-primary {
    color: rgb(var(--color-surface-50));
    border-color: color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    background: rgb(var(--color-primary-600));
  }

  :global(html:not(.dark)) .chat-artifact-action-primary {
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600));
    border-color: rgb(var(--color-primary-500) / 0.65);
  }

  .chat-artifact-action-primary:hover {
    background: rgb(var(--color-primary-500));
  }

  .chat-artifact-card-body {
    position: relative;
    z-index: 1;
  }

  .chat-artifact-launch {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 38%, transparent);
    border-radius: 0.875rem;
    padding: 0.75rem 0.875rem;
    text-align: left;
    background: rgb(var(--color-surface-900));
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.1);
    cursor: pointer;
    transition:
      transform 140ms ease,
      border-color 140ms ease,
      box-shadow 140ms ease;
  }

  :global(html:not(.dark)) .chat-artifact-launch {
    background: rgb(var(--color-surface-50));
    border-color: rgb(var(--color-surface-300) / 0.9);
    box-shadow: 0 8px 20px rgb(0 0 0 / 0.05);
  }

  .chat-artifact-launch:hover {
    transform: translateY(-1px);
    border-color: color-mix(in srgb, var(--color-primary-400) 35%, var(--color-surface-600));
    box-shadow: 0 12px 28px rgb(0 0 0 / 0.14);
  }

  .chat-artifact-launch-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.125rem;
  }

  .chat-artifact-launch-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--color-primary-400) 80%, var(--color-surface-500));
  }

  .chat-artifact-launch-label {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-surface-100);
  }

  :global(html:not(.dark)) .chat-artifact-launch-label {
    color: var(--color-surface-900);
  }

  .chat-artifact-launch-action {
    flex-shrink: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    color: var(--color-primary-300);
  }

  :global(html:not(.dark)) .chat-artifact-launch-action {
    color: rgb(var(--color-primary-600));
  }
</style>
