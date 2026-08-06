<script lang="ts">
  import type { ArtifactSummary } from "$lib/types/artifact";

  interface Props {
    artifacts: ArtifactSummary[];
    selectedArtifactId: string | null;
    onSelect: (artifactId: string) => void;
  }

  let { artifacts, selectedArtifactId, onSelect }: Props = $props();

  const sorted = $derived.by(() =>
    [...artifacts].sort((a, b) =>
      (b.stored_at_utc ?? "").localeCompare(a.stored_at_utc ?? ""),
    ),
  );

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

<div class="artifact-library-list flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 py-1">
  {#if sorted.length === 0}
    <p class="px-2 py-6 text-sm text-content-quiet">No artifacts yet.</p>
  {:else}
    <ul class="space-y-0.5">
      {#each sorted as artifact (artifact.artifact_id)}
        <li>
          <button
            type="button"
            class="artifact-library-item"
            class:artifact-library-item-active={selectedArtifactId === artifact.artifact_id}
            onclick={() => onSelect(artifact.artifact_id)}
          >
            <span class="artifact-library-item-title">{artifact.label}</span>
            <span class="artifact-library-item-meta">{formatWhen(artifact.stored_at_utc)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .artifact-library-item {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 0.125rem;
    border-radius: 0.5rem;
    padding: 0.45rem 0.6rem;
    text-align: left;
    background: transparent;
    cursor: pointer;
    transition: background 140ms ease;
  }

  .artifact-library-item:hover {
    background: rgb(var(--shell-pane-muted-bg) / 0.5);
  }

  .artifact-library-item-active {
    background: rgb(var(--theme-selection) / 0.14);
  }

  .artifact-library-item-active:hover {
    background: rgb(var(--theme-selection) / 0.2);
  }

  .artifact-library-item-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--theme-text));
  }

  .artifact-library-item-meta {
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-tertiary));
  }
</style>
