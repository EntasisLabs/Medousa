<script lang="ts">
  import { onMount } from "svelte";
  import { listUiArtifacts } from "$lib/daemon";
  import { environment } from "$lib/stores/environment.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import VaultKindBadge from "$lib/components/vault/VaultKindBadge.svelte";
  import type { ArtifactSummary } from "$lib/types/artifact";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import type { MediaEmbedProvider } from "$lib/utils/mediaEmbed";
  import { fuzzyMatchVaultNotes } from "$lib/utils/vaultFuzzyMatch";

  interface Props {
    defaultSurfaceId?: string | null;
    compact?: boolean;
    onAdded?: (componentId: string) => void;
  }

  let { defaultSurfaceId = null, compact = false, onAdded }: Props = $props();

  let tab = $state<"artifacts" | "media" | "notes">("artifacts");
  let query = $state("");
  let noteQuery = $state("");
  let artifacts = $state<ArtifactSummary[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let targetSurfaceId = $state("");
  let busyArtifactId = $state<string | null>(null);
  let actionError = $state<string | null>(null);

  let mediaProvider = $state<MediaEmbedProvider>("spotify");
  let mediaUrl = $state("");
  let mediaLabel = $state("");
  let mediaBusy = $state(false);
  let noteBusyPath = $state<string | null>(null);

  const customSurfaces = $derived(
    (environment.spec?.surfaces ?? []).filter((surface) => surface.kind === "custom"),
  );
  const noteLabelByPath = $derived(vault.labelByPathMap);
  const noteMatches = $derived(
    fuzzyMatchVaultNotes(vault.notes, noteQuery, noteLabelByPath, compact ? 20 : 30),
  );

  $effect(() => {
    if (defaultSurfaceId && customSurfaces.some((surface) => surface.id === defaultSurfaceId)) {
      targetSurfaceId = defaultSurfaceId;
    } else if (!targetSurfaceId && customSurfaces.length > 0) {
      targetSurfaceId = customSurfaces[0]?.id ?? "";
    }
  });

  onMount(() => {
    void refreshArtifacts();
    void vault.refreshNotes();
  });

  async function refreshArtifacts() {
    loading = true;
    loadError = null;
    try {
      const response = await listUiArtifacts({ limit: 100, query: query.trim() || undefined });
      artifacts = response.artifacts;
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
      artifacts = [];
    } finally {
      loading = false;
    }
  }

  async function addArtifact(artifact: ArtifactSummary) {
    if (!targetSurfaceId) {
      actionError = "Create a custom view first.";
      return;
    }
    busyArtifactId = artifact.artifact_id;
    actionError = null;
    try {
      const componentId = await environment.addPresentationFromArtifact({
        surfaceId: targetSurfaceId,
        artifactId: artifact.artifact_id,
        label: artifact.label,
      });
      onAdded?.(componentId);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyArtifactId = null;
    }
  }

  async function addMediaWidget() {
    if (!targetSurfaceId) {
      actionError = "Create a custom view first.";
      return;
    }
    mediaBusy = true;
    actionError = null;
    try {
      const componentId = await environment.addMediaEmbedWidget({
        surfaceId: targetSurfaceId,
        provider: mediaProvider,
        embedUrl: mediaUrl.trim(),
        label: mediaLabel.trim() || (mediaProvider === "spotify" ? "Spotify" : "Apple Music"),
      });
      mediaUrl = "";
      mediaLabel = "";
      onAdded?.(componentId);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      mediaBusy = false;
    }
  }

  async function addNoteWidget(notePath: string) {
    if (!targetSurfaceId) {
      actionError = "Create a custom view first.";
      return;
    }
    noteBusyPath = notePath;
    actionError = null;
    try {
      const componentId = await environment.addMedousaViewFromNote({
        surfaceId: targetSurfaceId,
        notePath,
        label: vaultDisplayTitle(noteLabelByPath.get(notePath) ?? notePath, notePath),
      });
      onAdded?.(componentId);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      noteBusyPath = null;
    }
  }

  function formatWhen(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }
</script>

<div class="canvas-widget-catalog" class:canvas-widget-catalog-compact={compact}>
  <header class="canvas-widget-catalog-header">
    <div>
      <h3 class="canvas-widget-catalog-title">Widget catalog</h3>
      <p class="workshop-faint text-xs">
        Add HTML presentations, vault notes, or native Spotify / Apple embeds.
      </p>
    </div>
    <label class="canvas-target-field">
      <span>Target view</span>
      <select bind:value={targetSurfaceId} disabled={customSurfaces.length === 0}>
        {#if customSurfaces.length === 0}
          <option value="">No custom views yet</option>
        {:else}
          {#each customSurfaces as surface (surface.id)}
            <option value={surface.id}>{surface.label}</option>
          {/each}
        {/if}
      </select>
    </label>
  </header>

  <div class="canvas-widget-tabs" role="tablist">
    <button
      type="button"
      role="tab"
      aria-selected={tab === "artifacts"}
      class="canvas-widget-tab"
      class:canvas-widget-tab-active={tab === "artifacts"}
      onclick={() => (tab = "artifacts")}
    >
      Presentations
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={tab === "media"}
      class="canvas-widget-tab"
      class:canvas-widget-tab-active={tab === "media"}
      onclick={() => (tab = "media")}
    >
      Spotify / Apple
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={tab === "notes"}
      class="canvas-widget-tab"
      class:canvas-widget-tab-active={tab === "notes"}
      onclick={() => (tab = "notes")}
    >
      Vault notes
    </button>
  </div>

  {#if tab === "artifacts"}
    <div class="canvas-widget-search-row">
      <input
        type="search"
        bind:value={query}
        placeholder="Search presentations…"
        onkeydown={(event) => {
          if (event.key === "Enter") void refreshArtifacts();
        }}
      />
      <button type="button" class="btn btn-xs btn-ghost" disabled={loading} onclick={() => void refreshArtifacts()}>
        {loading ? "…" : "Search"}
      </button>
    </div>

    {#if loadError}
      <p class="canvas-widget-error">{loadError}</p>
    {:else if loading}
      <p class="workshop-faint text-xs">Loading library…</p>
    {:else if artifacts.length === 0}
      <p class="workshop-faint text-xs">
        No presentations in your library yet — ask Medousa to build a widget, then pin it here.
      </p>
    {:else}
      <ul class="canvas-picker-list">
        {#each artifacts as artifact (artifact.artifact_id)}
          <li class="canvas-picker-tile">
            <div class="canvas-picker-tile-main">
              <p class="canvas-picker-tile-title">{artifact.label}</p>
              <p class="canvas-picker-tile-meta">
                {formatWhen(artifact.stored_at_utc)}
                {#if artifact.presentation}
                  · {artifact.presentation}
                {/if}
              </p>
            </div>
            <button
              type="button"
              class="canvas-picker-tile-btn"
              disabled={!targetSurfaceId || busyArtifactId === artifact.artifact_id}
              onclick={() => void addArtifact(artifact)}
            >
              {busyArtifactId === artifact.artifact_id ? "Adding…" : "Add"}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if tab === "media"}
    <form
      class="canvas-media-form"
      onsubmit={(event) => {
        event.preventDefault();
        void addMediaWidget();
      }}
    >
      <label class="canvas-target-field">
        <span>Provider</span>
        <select bind:value={mediaProvider} disabled={mediaBusy}>
          <option value="spotify">Spotify</option>
          <option value="apple_music">Apple Music</option>
        </select>
      </label>
      <label class="canvas-target-field">
        <span>Share or embed URL</span>
        <input
          type="url"
          bind:value={mediaUrl}
          placeholder="https://open.spotify.com/playlist/…"
          required
          disabled={mediaBusy}
        />
      </label>
      <label class="canvas-target-field">
        <span>Label (optional)</span>
        <input type="text" bind:value={mediaLabel} placeholder="Focus playlist" disabled={mediaBusy} />
      </label>
      <button type="submit" class="btn btn-sm btn-primary" disabled={mediaBusy || !targetSurfaceId || !mediaUrl.trim()}>
        {mediaBusy ? "Adding…" : "Add media widget"}
      </button>
    </form>
  {:else}
    <div class="canvas-widget-search-row">
      <input
        type="search"
        bind:value={noteQuery}
        placeholder="Search vault notes…"
      />
    </div>
    {#if vault.notes.length === 0}
      <p class="workshop-faint text-xs">No notes in your vault yet.</p>
    {:else if noteMatches.length === 0}
      <p class="workshop-faint text-xs">No notes match your search.</p>
    {:else}
      <ul class="canvas-picker-list">
        {#each noteMatches as note (note.path)}
          <li class="canvas-picker-tile">
            <div class="canvas-picker-tile-main">
              <p class="canvas-picker-tile-title">
                {vaultDisplayTitle(noteLabelByPath.get(note.path) ?? note.path, note.path)}
              </p>
              <div class="canvas-picker-tile-meta">
                <VaultKindBadge kind={note.kind} />
                <span class="canvas-picker-tile-path" title={note.path}>{note.path}</span>
              </div>
            </div>
            <button
              type="button"
              class="canvas-picker-tile-btn"
              disabled={!targetSurfaceId || noteBusyPath === note.path}
              onclick={() => void addNoteWidget(note.path)}
            >
              {noteBusyPath === note.path ? "Adding…" : "Add"}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if actionError}
    <p class="canvas-widget-error">{actionError}</p>
  {/if}
</div>

<style>
  .canvas-widget-catalog {
    margin-top: 1.25rem;
    padding: 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 30%, transparent);
    min-width: 0;
  }

  .canvas-widget-catalog-compact {
    margin-top: 0;
    border: 0;
    padding: 0;
    background: transparent;
  }

  .canvas-widget-catalog-header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .canvas-widget-catalog-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .canvas-target-field {
    display: grid;
    gap: 0.2rem;
    font-size: 0.6875rem;
    min-width: 10rem;
  }

  .canvas-target-field span {
    color: rgb(var(--color-surface-500));
  }

  .canvas-target-field select,
  .canvas-target-field input {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.3rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .canvas-target-field select:focus-visible,
  .canvas-target-field input:focus-visible {
    border-color: rgb(var(--color-primary-400));
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400));
  }

  .canvas-widget-tabs {
    display: flex;
    gap: 0.35rem;
    margin-bottom: 0.65rem;
  }

  .canvas-widget-tab {
    border-radius: 999px;
    padding: 0.2rem 0.55rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
    background: transparent;
    cursor: pointer;
  }

  .canvas-widget-tab-active {
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .canvas-widget-search-row {
    display: flex;
    gap: 0.35rem;
    margin-bottom: 0.5rem;
    padding-block: 2px;
  }

  .canvas-widget-search-row input {
    flex: 1 1 auto;
    min-width: 0;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .canvas-widget-search-row input:focus-visible {
    border-color: rgb(var(--color-primary-400));
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400));
  }

  .canvas-picker-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.45rem;
    max-height: 16rem;
    overflow-x: hidden;
    overflow-y: auto;
    min-width: 0;
  }

  .canvas-picker-tile {
    display: grid;
    gap: 0.5rem;
    padding: 0.6rem 0.65rem;
    border-radius: 0.6rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 45%, transparent);
    min-width: 0;
    overflow: hidden;
  }

  .canvas-picker-tile-main {
    min-width: 0;
  }

  .canvas-picker-tile-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 500;
    color: rgb(var(--color-surface-100));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .canvas-picker-tile-meta {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.25rem;
    min-width: 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .canvas-picker-tile-meta :global(.badge) {
    flex-shrink: 0;
  }

  .canvas-picker-tile-path {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .canvas-picker-tile-btn {
    justify-self: start;
    border: 0;
    border-radius: 999px;
    padding: 0.28rem 0.7rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600));
    cursor: pointer;
  }

  .canvas-picker-tile-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .canvas-media-form {
    display: grid;
    gap: 0.55rem;
    padding-block: 2px;
  }

  .canvas-widget-error {
    margin: 0.5rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
