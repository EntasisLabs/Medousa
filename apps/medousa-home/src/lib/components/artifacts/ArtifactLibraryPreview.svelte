<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import ArtifactExportMenu from "$lib/components/chat/ArtifactExportMenu.svelte";
  import ArtifactFullscreen from "$lib/components/chat/ArtifactFullscreen.svelte";
  import ArtifactPanel from "$lib/components/chat/ArtifactPanel.svelte";
  import type { ArtifactSummary } from "$lib/types/artifact";
  import { artifactSummaryToUi } from "$lib/types/artifact";
  import { Expand, MessageSquare } from "@lucide/svelte";

  interface Props {
    artifact: ArtifactSummary | null;
    sessionTitle: string;
    panelOpen?: boolean;
    onOpenChat: () => void;
    onOpenSession: (sessionId: string) => void;
  }

  let {
    artifact,
    sessionTitle,
    panelOpen = $bindable(false),
    onOpenChat,
    onOpenSession,
  }: Props = $props();
  let fullscreenOpen = $state(false);
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

  const uiArtifact = $derived.by(() =>
    artifact ? artifactSummaryToUi(artifact) : null,
  );
  const sourceLabel = $derived(
    artifact && sessionTitle.trim() && sessionTitle.trim() !== artifact.session_id
      ? sessionTitle.trim()
      : "Created by Medousa",
  );
</script>

<div class="artifact-library-preview flex h-full min-h-0 min-w-0 flex-1 flex-col">
  {#if !artifact || !uiArtifact}
    <div class="flex flex-1 items-center justify-center p-6 text-sm text-content-quiet">
      Select an artifact to preview.
    </div>
  {:else}
    <header class="artifact-library-preview-header">
      <div class="min-w-0">
        <h2 class="artifact-library-preview-title">{artifact.label}</h2>
        <p class="artifact-library-preview-source">
          {#if exportStatus}
            <span class="text-content-link">{exportStatus}</span>
          {:else}
            {sourceLabel}
          {/if}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <ArtifactExportMenu
          sessionId={artifact.session_id}
          artifactId={artifact.artifact_id}
          label={artifact.label}
          compact={true}
          onStatus={handleExportStatus}
        />
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
            fullscreenOpen = true;
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
        rootArtifactId={artifact.root_artifact_id}
        mode="panel"
        bare={true}
        manageable={true}
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
    min-height: 3.25rem;
    gap: 0.75rem;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.28);
    padding: 0.55rem 0.85rem;
  }

  .artifact-library-preview-title,
  .artifact-library-preview-source {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artifact-library-preview-title {
    margin: 0;
    color: rgb(var(--theme-text));
    font-size: 0.8125rem;
    font-weight: 550;
    letter-spacing: -0.01em;
  }

  .artifact-library-preview-source {
    margin: 0.12rem 0 0;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.625rem;
  }

  .artifact-library-preview-body {
    display: flex;
    min-height: 0;
    flex: 1 1 auto;
    flex-direction: column;
    padding: 0.65rem 0.85rem 0.85rem;
  }

  .artifact-library-action {
    display: inline-flex;
    align-items: center;
    gap: 0.32rem;
    min-height: 1.75rem;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    padding: 0.3rem 0.5rem !important;
    font-size: 0.65625rem;
    font-weight: 500;
    color: rgb(var(--theme-text-secondary));
    background: transparent;
    cursor: pointer;
    transition: background 130ms ease, color 130ms ease, border-color 130ms ease;
  }

  .artifact-library-action:hover {
    color: rgb(var(--theme-text));
    background: rgb(var(--shell-pane-muted-bg) / 0.55);
  }

  .artifact-library-action-primary {
    color: rgb(var(--theme-text-secondary));
    border-color: rgb(var(--theme-border) / 0.32);
    background: rgb(var(--shell-pane-muted-bg) / 0.32);
  }
</style>
