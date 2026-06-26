<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import ArtifactFullscreen from "$lib/components/chat/ArtifactFullscreen.svelte";
  import ArtifactPanel from "$lib/components/chat/ArtifactPanel.svelte";
  import type { ArtifactSummary } from "$lib/types/artifact";
  import { artifactSummaryToUi } from "$lib/types/artifact";
  import { Expand, MessageSquare } from "@lucide/svelte";

  interface Props {
    artifact: ArtifactSummary | null;
    sessionTitle: string;
    onOpenChat: () => void;
    onOpenSession: (sessionId: string) => void;
  }

  let { artifact, sessionTitle, onOpenChat, onOpenSession }: Props = $props();

  let panelOpen = $state(false);
  let fullscreenOpen = $state(false);

  const uiArtifact = $derived.by(() =>
    artifact ? artifactSummaryToUi(artifact) : null,
  );
</script>

<div class="artifact-library-preview flex h-full min-h-0 flex-col">
  {#if !artifact || !uiArtifact}
    <div class="flex flex-1 items-center justify-center p-6 text-sm text-surface-500">
      Select a presentation to preview.
    </div>
  {:else}
    <header class="artifact-library-preview-header">
      <div class="min-w-0">
        <h2 class="truncate text-sm font-semibold text-surface-100">{artifact.label}</h2>
        <p class="truncate text-xs text-surface-500">{sessionTitle}</p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="artifact-library-action"
          onclick={() => {
            onOpenSession(artifact.session_id);
            onOpenChat();
          }}
        >
          <MessageSquare size={14} aria-hidden="true" />
          Open chat
        </button>
        <button
          type="button"
          class="artifact-library-action artifact-library-action-primary"
          onclick={() => {
            panelOpen = true;
          }}
        >
          <Expand size={14} aria-hidden="true" />
          Expand
        </button>
      </div>
    </header>

    <div class="artifact-library-preview-body">
      <ArtifactEmbed
        sessionId={artifact.session_id}
        artifactId={artifact.artifact_id}
        label={artifact.label}
        mime="text/html"
        mode="panel"
        bare={true}
      />
    </div>

    <ArtifactPanel
      open={panelOpen}
      sessionId={artifact.session_id}
      artifact={uiArtifact}
      onClose={() => {
        panelOpen = false;
      }}
      onExpand={() => {
        panelOpen = false;
        fullscreenOpen = true;
      }}
    />

    <ArtifactFullscreen
      open={fullscreenOpen}
      sessionId={artifact.session_id}
      artifact={uiArtifact}
      onClose={() => {
        fullscreenOpen = false;
      }}
    />
  {/if}
</div>

<style>
  .artifact-library-preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
    padding: 0.75rem 1rem;
  }

  .artifact-library-preview-body {
    display: flex;
    min-height: 0;
    flex: 1 1 auto;
    flex-direction: column;
    padding: 0.75rem 1rem 1rem;
  }

  .artifact-library-action {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 65%, transparent);
    border-radius: 999px;
    padding: 0.35rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 72%, var(--color-surface-900));
    cursor: pointer;
  }

  .artifact-library-action-primary {
    color: rgb(var(--color-surface-50));
    border-color: color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    background: rgb(var(--color-primary-600));
  }
</style>
