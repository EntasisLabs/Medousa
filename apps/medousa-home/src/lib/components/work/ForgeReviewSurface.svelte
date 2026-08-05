<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import { getReviewFile, type ReviewFileDiff, type ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
    onSelectCandidate?: (attemptId: string) => void | Promise<void>;
  }

  let { review, busy = false, onOpenFile, onRestore, onSelectCandidate }: Props = $props();

  let mode = $state<"inline" | "side">("inline");
  let loading = $state(false);
  let error = $state<string | null>(null);
  let fileDiffs = $state<ReviewFileDiff[]>([]);
  let loadToken = 0;

  const stackFiles = $derived(fileDiffs.map(toStackFile));

  function toStackFile(diff: ReviewFileDiff): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    return {
      path: diff.path,
      oldPath: diff.old_path,
      status: diff.status,
      binary: diff.binary,
      additions: stats.additions,
      deletions: stats.deletions,
      hunks: diff.hunks,
      baselineBytes: diff.baseline.byte_size,
      reviewedBytes: diff.reviewed.byte_size,
      baselineExists: diff.baseline.exists,
      reviewedExists: diff.reviewed.exists,
    };
  }

  async function loadStack(paths: string[], workId: string, attemptId?: string | null) {
    const token = ++loadToken;
    loading = true;
    error = null;
    try {
      const settled = await Promise.all(
        paths.map(async (path) => {
          try {
            return await getReviewFile(workId, path, attemptId ?? undefined);
          } catch (err) {
            return {
              error: err instanceof Error ? err.message : String(err),
              path,
            } as const;
          }
        }),
      );
      if (token !== loadToken) return;
      const ok: ReviewFileDiff[] = [];
      const failures: string[] = [];
      for (const item of settled) {
        if ("error" in item) failures.push(`${item.path}: ${item.error}`);
        else ok.push(item);
      }
      fileDiffs = ok;
      error = failures.length > 0 ? `Couldn’t load ${failures.length} comparison(s).` : null;
    } finally {
      if (token === loadToken) loading = false;
    }
  }

  $effect(() => {
    const workId = review.work_id;
    const attemptId = review.attempt_id;
    const paths = review.changed_files.map((file) => file.path);
    void loadStack(paths, workId, attemptId);
  });

  async function restorePath(path: string) {
    const comparison = fileDiffs.find((file) => file.path === path);
    if (!comparison) return;
    await onRestore(comparison);
  }
</script>

<section class="review-surface" aria-label="Change review">
  {#if review.candidates.length > 1}
    <label class="review-candidate-picker">
      <span>Review candidate</span>
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
  {/if}

  <div class="review-summary">
    <div class="review-summary-copy">
      <p class="review-summary-kicker">Ready for review</p>
      <p class="review-summary-text">{review.synthesis.status_summary}</p>
    </div>
    <div class="review-signals" aria-label="Review signals">
      <span
        class="review-signal review-signal--{review.synthesis.risk}"
        title={review.synthesis.risk_summary}
      >{review.synthesis.risk} risk</span>
      <span
        class="review-signal {review.synthesis.verification?.success
          ? 'review-signal--passed'
          : 'review-signal--unchecked'}"
        title={review.synthesis.verification
          ? `${review.synthesis.verification.label} · ${review.synthesis.verification.command.join(" ")}`
          : "No project check was recorded"}
      >{review.synthesis.verification?.success ? "Checked" : "Not checked"}</span>
    </div>
  </div>

  {#if review.attribution.length > 0}
    <div class="review-attribution" aria-label="Who contributed">
      {#each review.attribution as actor (actor.id)}
        <span class="review-attribution-chip review-attribution-chip--{actor.kind}">
          {actor.label}
          {#if actor.files.length > 0}
            <span>· {actor.files.length}</span>
          {/if}
        </span>
      {/each}
    </div>
  {/if}

  {#if loading && fileDiffs.length === 0}
    <div class="review-loading">
      <LoaderCircle size={14} class="animate-spin" />
      <span>Preparing comparisons…</span>
    </div>
  {:else if error && fileDiffs.length === 0}
    <div class="review-error">
      <p>Couldn’t prepare this comparison.</p>
      <span>{error}</span>
    </div>
  {:else}
    {#if error}
      <p class="review-partial-error">{error}</p>
    {/if}
    <DiffStack
      files={stackFiles}
      bind:mode
      busy={busy || loading}
      subtitle={review.synthesis.recommended_next_action || undefined}
      onOpenFile={(path, line) => void onOpenFile(path, line)}
      onRestoreFile={(path) => void restorePath(path)}
    />
  {/if}
</section>

<style>
  .review-surface {
    color: rgb(var(--color-surface-100));
  }

  .review-candidate-picker {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.65rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.6875rem;
  }

  .review-candidate-picker select {
    min-width: min(22rem, 70%);
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-900));
    padding: 0.35rem 0.5rem;
    color: rgb(var(--color-surface-200));
  }

  .review-summary {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1.5rem;
    padding: 0.35rem 0.15rem 0.75rem;
  }

  .review-summary-copy {
    min-width: 0;
    max-width: 48rem;
  }

  .review-summary-kicker {
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-200));
  }

  .review-summary-text {
    margin-top: 0.2rem;
    font-size: 0.75rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-500));
  }

  .review-signals {
    display: flex;
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .review-signal {
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    font-size: 0.5625rem;
    font-weight: 500;
    color: rgb(var(--color-surface-300));
  }

  .review-signal--high {
    border-color: rgb(var(--color-error-500) / 0.35);
    color: rgb(251 207 232);
  }

  .review-signal--attention {
    border-color: rgb(245 158 11 / 0.35);
    color: rgb(253 230 138);
  }

  .review-signal--passed {
    border-color: rgb(52 211 153 / 0.35);
    color: rgb(167 243 208);
  }

  .review-attribution {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-bottom: 0.75rem;
  }

  .review-attribution-chip {
    border-radius: 999px;
    border: 1px solid rgb(var(--color-surface-500) / 0.25);
    padding: 0.15rem 0.5rem;
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-400));
  }

  .review-attribution-chip--human {
    color: rgb(186 230 253);
  }

  .review-attribution-chip--agent {
    color: rgb(216 180 254);
  }

  .review-attribution-chip--terminal {
    color: rgb(167 243 208);
  }

  .review-loading,
  .review-error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.35rem;
    padding: 1.25rem 0.5rem;
    color: rgb(var(--color-surface-400));
    font-size: 0.75rem;
  }

  .review-error span,
  .review-partial-error {
    color: rgb(253 230 138);
    font-size: 0.625rem;
  }

  .review-partial-error {
    margin: 0 0 0.5rem;
  }
</style>
