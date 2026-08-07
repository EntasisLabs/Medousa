<script lang="ts">
  import ReviewFileChanges from "$lib/components/work/ReviewFileChanges.svelte";
  import ReviewIntentHeader from "$lib/components/work/ReviewIntentHeader.svelte";
  import type { ReviewFileDiff, ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
    onSelectCandidate?: (attemptId: string) => void | Promise<void>;
    onComment?: (input: {
      path: string;
      side: "new" | "old";
      line: number;
      content: string;
    }) => void;
    onToggleCommentRail?: () => void;
  }

  let {
    review,
    busy = false,
    onOpenFile,
    onRestore,
    onSelectCandidate,
    onComment,
  }: Props = $props();

  const followUp = $derived((review.changed_since_previous?.length ?? 0) > 0);
</script>

<div class="review-surface" aria-label="Change review">
  {#if review.candidates.length > 1}
    <details class="review-candidates">
      <summary>Compare another sealed attempt</summary>
      <label>
        <span class="sr-only">Review candidate</span>
        <select
          value={review.attempt_id ?? ""}
          disabled={busy}
          onchange={(event) => void onSelectCandidate?.(event.currentTarget.value)}
        >
          {#each review.candidates as candidate (candidate.attempt_id)}
            <option value={candidate.attempt_id}>
              Attempt {candidate.attempt_seq} · {candidate.executor} · {candidate.changed_file_count} files
            </option>
          {/each}
        </select>
      </label>
    </details>
  {/if}

  <ReviewIntentHeader {review} {followUp} />

  {#if review.evidence_id && review.changed_files.length === 0}
    <div class="review-empty-seal" role="status">
      <p>No file changes in this revision</p>
      <span>Nothing was sealed — or pick another attempt if several exist.</span>
    </div>
  {:else}
    <ReviewFileChanges {review} {busy} {onOpenFile} {onRestore} {onComment} />
  {/if}
</div>

<style>
  .review-surface {
    color: rgb(var(--color-surface-100));
  }

  .review-candidates {
    margin-bottom: 0.75rem;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-candidates summary {
    cursor: pointer;
    list-style: none;
  }

  .review-candidates summary::-webkit-details-marker {
    display: none;
  }

  .review-candidates select {
    margin-top: 0.35rem;
    min-width: min(22rem, 70%);
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-900));
    padding: 0.35rem 0.5rem;
    color: rgb(var(--color-surface-200));
  }

  .review-empty-seal {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 1rem 0.25rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .review-empty-seal p {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-200));
  }

  .review-empty-seal span {
    font-size: 0.75rem;
    line-height: 1.45;
  }

  :global(.sr-only) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
