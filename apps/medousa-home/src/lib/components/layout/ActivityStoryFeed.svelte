<script lang="ts">
  import type { ActivityStoryChapter } from "$lib/utils/activityStory";

  interface Props {
    chapters: ActivityStoryChapter[];
    emptyHidden?: boolean;
    compact?: boolean;
  }

  let { chapters, emptyHidden = false, compact = false }: Props = $props();
</script>

{#if chapters.length === 0}
  <div class="activity-story-empty" class:activity-story-empty--compact={compact}>
    <p class="activity-story-empty-title">All quiet</p>
    <p class="activity-story-empty-copy">
      {#if emptyHidden}
        Cleared on this device — new work still lands here.
      {:else}
        Saves and work gather here as a short story.
      {/if}
    </p>
  </div>
{:else}
  {#each chapters as chapter (chapter.key)}
    <section class="activity-story-chapter" class:activity-story-chapter--compact={compact}>
      <h3 class="activity-story-chapter-label">{chapter.label}</h3>
      <ol class="activity-story-list">
        {#each chapter.beats as beat (beat.id)}
          <li class="activity-story-beat" class:activity-story-beat--compact={compact}>
            {#if beat.kicker}
              <p class="activity-story-beat-kicker">{beat.kicker}</p>
            {/if}
            <div class="activity-story-beat-row">
              <p class="activity-story-beat-summary">{beat.presentation.summary}</p>
              <time
                class="activity-story-beat-time"
                datetime={beat.event.timestamp_utc}
              >
                {beat.presentation.time}
              </time>
            </div>
            {#if beat.presentation.context}
              <p class="activity-story-beat-context">{beat.presentation.context}</p>
            {/if}
          </li>
        {/each}
      </ol>
    </section>
  {/each}
{/if}

<style>
  .activity-story-empty {
    padding: 3rem 0.5rem;
    text-align: center;
  }

  .activity-story-empty--compact {
    padding: 1.25rem 0.75rem;
  }

  .activity-story-empty-title {
    margin: 0;
    font-size: 0.875rem;
    color: rgb(var(--shell-label, var(--color-surface-300)));
  }

  .activity-story-empty-copy {
    margin: 0.4rem 0 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .activity-story-chapter {
    margin-bottom: 1.45rem;
  }

  .activity-story-chapter--compact {
    margin-bottom: 0.85rem;
  }

  .activity-story-chapter-label {
    margin: 0 0 0.55rem;
    padding: 0 0.4rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.9);
  }

  .activity-story-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .activity-story-beat {
    padding: 0.7rem 0.45rem;
    border-radius: 0.55rem;
    transition: background 140ms ease;
  }

  .activity-story-beat--compact {
    padding: 0.45rem 0.35rem;
  }

  .activity-story-beat + .activity-story-beat {
    margin-top: 0.15rem;
  }

  .activity-story-beat:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.28);
  }

  .activity-story-beat-kicker {
    margin: 0 0 0.2rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .activity-story-beat-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.85rem;
  }

  .activity-story-beat-time {
    flex-shrink: 0;
    font-size: 0.625rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.85);
  }

  .activity-story-beat-summary {
    margin: 0;
    min-width: 0;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.4;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
    overflow-wrap: anywhere;
  }

  .activity-story-beat--compact .activity-story-beat-summary {
    font-size: 0.75rem;
  }

  .activity-story-beat-context {
    margin: 0.25rem 0 0;
    font-size: 0.6875rem;
    line-height: 1.4;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.92);
    overflow-wrap: anywhere;
  }
</style>
