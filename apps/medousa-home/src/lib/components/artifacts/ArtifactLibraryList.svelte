<script lang="ts">
  import { LayoutTemplate } from "@lucide/svelte";
  import { artifactSessionTitle } from "$lib/runtime/artifactSessionTitlePort";
  import type { ArtifactSummary } from "$lib/types/artifact";

  interface Props {
    artifacts: ArtifactSummary[];
    selectedArtifactId: string | null;
    onSelect: (artifactId: string) => void;
    emptyLabel?: string;
  }

  type ArtifactGroup = { label: string; artifacts: ArtifactSummary[] };

  let {
    artifacts,
    selectedArtifactId,
    onSelect,
    emptyLabel = "No artifacts yet.",
  }: Props = $props();

  const sorted = $derived.by(() =>
    [...artifacts].sort((a, b) =>
      (b.stored_at_utc ?? "").localeCompare(a.stored_at_utc ?? ""),
    ),
  );

  const grouped = $derived.by((): ArtifactGroup[] => {
    const groups = new Map<string, ArtifactSummary[]>();
    for (const artifact of sorted) {
      const label = dateGroup(artifact.stored_at_utc);
      const entries = groups.get(label) ?? [];
      entries.push(artifact);
      groups.set(label, entries);
    }
    return Array.from(groups, ([label, entries]) => ({ label, artifacts: entries }));
  });

  function parsedDate(value: string): Date | null {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  function startOfDay(date: Date): number {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  }

  function dateGroup(value: string): string {
    const date = parsedDate(value);
    if (!date) return "Earlier";
    const now = new Date();
    const daysAgo = Math.floor((startOfDay(now) - startOfDay(date)) / 86_400_000);
    if (daysAgo <= 0) return "Today";
    if (daysAgo <= 7) return "Previous 7 days";
    return date.toLocaleDateString(undefined, { month: "long", year: "numeric" });
  }

  function formatWhen(value: string): string {
    const date = parsedDate(value);
    if (!date) return value;
    const now = new Date();
    if (startOfDay(now) === startOfDay(date)) {
      return date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    }
    return date.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
    });
  }

  function sourceLabel(artifact: ArtifactSummary): string {
    const title = artifactSessionTitle(artifact.session_id).trim();
    if (!title || title === artifact.session_id) return "Medousa presentation";
    return title;
  }
</script>

<div class="artifact-library-list">
  {#if sorted.length === 0}
    <div class="artifact-library-empty">
      <span class="artifact-library-empty-icon" aria-hidden="true">
        <LayoutTemplate size={16} strokeWidth={1.6} />
      </span>
      <p>{emptyLabel}</p>
      <span>Presentations Medousa creates will appear here.</span>
    </div>
  {:else}
    {#each grouped as group (group.label)}
      <section class="artifact-library-group">
        <h3 class="artifact-library-group-label">
          <span>{group.label}</span>
          <span>{group.artifacts.length}</span>
        </h3>
        <ul class="artifact-library-group-list">
          {#each group.artifacts as artifact (artifact.artifact_id)}
            <li>
              <button
                type="button"
                class="artifact-library-item"
                class:artifact-library-item-active={selectedArtifactId === artifact.artifact_id}
                title={artifact.label}
                onclick={() => onSelect(artifact.artifact_id)}
              >
                <span class="artifact-library-item-icon" aria-hidden="true">
                  <LayoutTemplate size={14} strokeWidth={1.65} />
                </span>
                <span class="artifact-library-item-copy">
                  <span class="artifact-library-item-title">{artifact.label}</span>
                  <span class="artifact-library-item-source">{sourceLabel(artifact)}</span>
                </span>
                <span class="artifact-library-item-meta">{formatWhen(artifact.stored_at_utc)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</div>

<style>
  .artifact-library-list {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 0.35rem 0.4rem 0.8rem;
    scrollbar-width: thin;
    scrollbar-color: rgb(var(--theme-border) / 0.45) transparent;
  }

  .artifact-library-group + .artifact-library-group { margin-top: 0.65rem; }

  .artifact-library-group-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.35rem 0.5rem 0.28rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.5625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    line-height: 1;
    text-transform: uppercase;
  }

  .artifact-library-group-label span:last-child {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0;
    opacity: 0.72;
  }

  .artifact-library-group-list { display: flex; flex-direction: column; gap: 1px; }

  .artifact-library-item {
    display: flex;
    width: 100%;
    min-height: 2.85rem;
    align-items: center;
    gap: 0.55rem;
    border-radius: 0.45rem;
    padding: 0.42rem 0.48rem !important;
    text-align: left;
    background: transparent;
    cursor: pointer;
    transition: background 130ms ease, box-shadow 130ms ease;
  }

  .artifact-library-item:hover { background: rgb(var(--shell-pane-muted-bg) / 0.48); }

  .artifact-library-item-active {
    background: rgb(var(--theme-selection) / 0.13);
    box-shadow: inset 0 0 0 1px rgb(var(--theme-border) / 0.22);
  }

  .artifact-library-item-active:hover { background: rgb(var(--theme-selection) / 0.17); }

  .artifact-library-item-icon {
    display: inline-flex;
    width: 1.65rem;
    height: 1.65rem;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border-radius: 0.32rem;
    color: rgb(var(--theme-text-tertiary));
    background: rgb(var(--shell-pane-muted-bg) / 0.5);
  }

  .artifact-library-item-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.16rem;
  }

  .artifact-library-item-title,
  .artifact-library-item-source {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artifact-library-item-title {
    color: rgb(var(--theme-text));
    font-size: 0.78125rem;
    font-weight: 500;
    line-height: 1.15;
    letter-spacing: -0.008em;
  }

  .artifact-library-item-source,
  .artifact-library-item-meta {
    color: rgb(var(--theme-text-quiet));
    font-size: 0.59375rem;
    line-height: 1.15;
  }

  .artifact-library-item-meta {
    flex: 0 0 auto;
    align-self: flex-start;
    padding-top: 0.08rem;
    font-variant-numeric: tabular-nums;
  }

  .artifact-library-empty {
    display: flex;
    min-height: 11rem;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    padding: 1.5rem;
    text-align: center;
    color: rgb(var(--theme-text-tertiary));
  }

  .artifact-library-empty-icon {
    display: inline-flex;
    width: 2rem;
    height: 2rem;
    align-items: center;
    justify-content: center;
    margin-bottom: 0.65rem;
    border-radius: 0.5rem;
    background: rgb(var(--shell-pane-muted-bg) / 0.55);
  }

  .artifact-library-empty p {
    margin: 0;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
    font-weight: 500;
  }

  .artifact-library-empty > span:last-child {
    max-width: 12rem;
    margin-top: 0.25rem;
    font-size: 0.625rem;
    line-height: 1.4;
  }
</style>
