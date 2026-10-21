<script lang="ts">
  import type { ReviewChangeKind } from "$lib/code/reviewChangeKinds";
  import type { ReviewReadingState } from "$lib/code/reviewPage.svelte";
  import ReviewFileChanges from "$lib/components/work/ReviewFileChanges.svelte";
  import type { ReviewFileDiff, ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    directory?: string | null;
    changeKind?: ReviewChangeKind | null;
    viewState?: ReviewReadingState["diff"];
    onReady?: () => void;
    projectTitle?: string | null;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore?: (comparison: ReviewFileDiff) => Promise<void>;
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
    directory = null,
    changeKind = null,
    viewState,
    onReady,
    projectTitle: _projectTitle = null,
    busy = false,
    onOpenFile,
    onRestore,
    onSelectCandidate: _onSelectCandidate,
    onComment,
  }: Props = $props();
</script>

<div class="review-surface" aria-label="Change review">


  {#if review.evidence_id && review.changed_files.length === 0}
    <div class="review-empty-seal" role="status">
      <p>No file changes in this revision</p>
      <span>Nothing was sealed — or pick another attempt from project actions if several exist.</span>
    </div>
  {:else}
    <ReviewFileChanges {changeKind} {directory} {viewState} {onReady} {review} {busy} {onOpenFile} {onRestore} {onComment} />
  {/if}
</div>

<style>
  .review-surface {
    color: rgb(var(--theme-text));
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
    color: rgb(var(--theme-text));
  }

  .review-empty-seal span {
    font-size: 0.75rem;
    line-height: 1.45;
  }
</style>
