<script lang="ts">
  import type { ArtifactSummary } from "$lib/types/artifact";

  interface Props {
    artifacts: ArtifactSummary[];
    selectedArtifactId: string | null;
    sessionTitle: (sessionId: string) => string;
    onSelect: (artifactId: string) => void;
  }

  let { artifacts, selectedArtifactId, sessionTitle, onSelect }: Props = $props();

  const grouped = $derived.by(() => {
    const map = new Map<string, ArtifactSummary[]>();
    for (const artifact of artifacts) {
      const bucket = map.get(artifact.session_id) ?? [];
      bucket.push(artifact);
      map.set(artifact.session_id, bucket);
    }
    return [...map.entries()].sort((a, b) => {
      const aTime = a[1][0]?.stored_at_utc ?? "";
      const bTime = b[1][0]?.stored_at_utc ?? "";
      return bTime.localeCompare(aTime);
    });
  });

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

<div class="artifact-library-list flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
  {#if artifacts.length === 0}
    <p class="px-2 py-6 text-sm text-surface-500">No presentations yet.</p>
  {:else}
    {#each grouped as [sessionId, items] (sessionId)}
      <div class="mb-3">
        <p class="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
          {sessionTitle(sessionId)}
        </p>
        <ul class="space-y-0.5">
          {#each items as artifact (artifact.artifact_id)}
            <li>
              <button
                type="button"
                class="artifact-library-item"
                class:artifact-library-item-active={selectedArtifactId === artifact.artifact_id}
                onclick={() => onSelect(artifact.artifact_id)}
              >
                <span class="artifact-library-item-title">{artifact.label}</span>
                <span class="artifact-library-item-meta">
                  {formatWhen(artifact.stored_at_utc)}
                  {#if artifact.presentation}
                    · {artifact.presentation}
                  {/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  {/if}
</div>

<style>
  .artifact-library-item {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 0.15rem;
    border-radius: 0.625rem;
    padding: 0.55rem 0.65rem;
    text-align: left;
    background: transparent;
    cursor: pointer;
    transition: background 140ms ease;
  }

  .artifact-library-item:hover {
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .artifact-library-item-active {
    background: rgb(var(--color-primary-600) / 0.14);
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-500) / 0.28);
  }

  .artifact-library-item-title {
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  :global(html:not(.dark)) .artifact-library-item-title {
    color: rgb(var(--color-surface-900));
  }

  .artifact-library-item-meta {
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }
</style>
