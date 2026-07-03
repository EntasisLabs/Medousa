<script lang="ts">
  import { onMount } from "svelte";
  import { listUiArtifacts } from "$lib/daemon";
  import { environment } from "$lib/stores/environment.svelte";
  import type { ArtifactSummary } from "$lib/types/artifact";
  import type { MediaEmbedProvider } from "$lib/utils/mediaEmbed";

  interface Props {
    defaultSurfaceId?: string | null;
    compact?: boolean;
    onAdded?: (componentId: string) => void;
  }

  let { defaultSurfaceId = null, compact = false, onAdded }: Props = $props();

  let tab = $state<"artifacts" | "media">("artifacts");
  let query = $state("");
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

  const customSurfaces = $derived(
    (environment.spec?.surfaces ?? []).filter((surface) => surface.kind === "custom"),
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
        Add HTML presentations from your library or native Spotify / Apple embeds.
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
      <ul class="canvas-artifact-list">
        {#each artifacts as artifact (artifact.artifact_id)}
          <li class="canvas-artifact-row">
            <div class="min-w-0">
              <p class="canvas-artifact-title">{artifact.label}</p>
              <p class="workshop-faint text-xs">
                {formatWhen(artifact.stored_at_utc)}
                {#if artifact.presentation}
                  · {artifact.presentation}
                {/if}
              </p>
            </div>
            <button
              type="button"
              class="btn btn-xs btn-primary"
              disabled={!targetSurfaceId || busyArtifactId === artifact.artifact_id}
              onclick={() => void addArtifact(artifact)}
            >
              {busyArtifactId === artifact.artifact_id ? "Adding…" : "Add"}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
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
  }

  .canvas-widget-search-row input {
    flex: 1 1 auto;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .canvas-artifact-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.35rem;
    max-height: 16rem;
    overflow: auto;
  }

  .canvas-artifact-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.45rem 0.5rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .canvas-artifact-title {
    margin: 0;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .canvas-media-form {
    display: grid;
    gap: 0.55rem;
  }

  .canvas-widget-error {
    margin: 0.5rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
