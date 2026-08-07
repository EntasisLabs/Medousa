<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import { getReviewFile, type ReviewFileDiff, type ReviewProjection } from "$lib/forge";
  import { isEditableTarget, modalBlocksHotkeys } from "$lib/utils/shellPaneHotkeys";

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
  }

  let { review, busy = false, onOpenFile, onRestore, onSelectCandidate, onComment }: Props =
    $props();

  let mode = $state<"inline" | "side">("inline");
  let density = $state<"comfortable" | "compact">("comfortable");
  let loading = $state(false);
  let fileDiffs = $state<ReviewFileDiff[]>([]);
  let fileErrors = $state<Record<string, string>>({});
  let loadToken = 0;
  let viewedPaths = $state<string[]>([]);
  let collapsedPaths = $state<string[]>([]);
  let reviewRoot: HTMLElement | null = $state(null);
  let focusIndex = $state(0);

  const riskPaths = $derived.by(() => {
    const paths = new Set<string>();
    for (const violation of review.policy?.violations ?? []) {
      if (violation.path) paths.add(violation.path);
    }
    for (const risk of review.policy?.capture_risks ?? []) {
      if ("path" in risk && typeof risk.path === "string") paths.add(risk.path);
    }
    return paths;
  });

  const orderedPaths = $derived.by(() => {
    const paths = review.changed_files.map((file) => file.path);
    return [...paths].sort((a, b) => {
      const ar = riskPaths.has(a) ? 0 : 1;
      const br = riskPaths.has(b) ? 0 : 1;
      if (ar !== br) return ar - br;
      return a.localeCompare(b);
    });
  });

  const stackFiles = $derived(
    orderedPaths
      .map((path) => fileDiffs.find((diff) => diff.path === path))
      .filter((diff): diff is ReviewFileDiff => Boolean(diff))
      .map(toStackFile),
  );

  const viewedCount = $derived(
    orderedPaths.filter((path) => viewedPaths.includes(path)).length,
  );

  function toStackFile(diff: ReviewFileDiff): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    return {
      id: diff.path,
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
      beforeText: diff.baseline.content ?? null,
      afterText: diff.reviewed.content ?? null,
    };
  }

  function viewedStorageKey(evidenceId: string | null | undefined): string | null {
    if (!evidenceId) return null;
    return `medousa-review-viewed:${evidenceId}`;
  }

  function densityStorageKey(): string {
    return "medousa-review-density";
  }

  function loadViewed(evidenceId: string | null | undefined) {
    const key = viewedStorageKey(evidenceId);
    if (!key || typeof localStorage === "undefined") {
      viewedPaths = [];
      return;
    }
    try {
      const raw = localStorage.getItem(key);
      viewedPaths = raw ? (JSON.parse(raw) as string[]) : [];
    } catch {
      viewedPaths = [];
    }
  }

  function persistViewed(evidenceId: string | null | undefined, paths: string[]) {
    const key = viewedStorageKey(evidenceId);
    if (!key || typeof localStorage === "undefined") return;
    localStorage.setItem(key, JSON.stringify(paths));
  }

  async function loadFile(path: string, workId: string, attemptId?: string | null, token?: number) {
    try {
      const diff = await getReviewFile(workId, path, attemptId ?? undefined);
      if (token != null && token !== loadToken) return;
      fileDiffs = [...fileDiffs.filter((file) => file.path !== path), diff];
      const { [path]: _, ...rest } = fileErrors;
      fileErrors = rest;
    } catch (err) {
      if (token != null && token !== loadToken) return;
      fileErrors = {
        ...fileErrors,
        [path]: err instanceof Error ? err.message : String(err),
      };
    }
  }

  async function loadStack(paths: string[], workId: string, attemptId?: string | null) {
    const token = ++loadToken;
    loading = true;
    fileDiffs = [];
    fileErrors = {};
    // Stream: kick all loads, render as each resolves.
    await Promise.all(
      paths.map(async (path) => {
        await loadFile(path, workId, attemptId, token);
      }),
    );
    if (token === loadToken) loading = false;

    // Default collapse for large stacks: collapse files after the first 8.
    if (paths.length > 12) {
      collapsedPaths = paths.slice(8);
    } else {
      collapsedPaths = [];
    }
  }

  $effect(() => {
    const workId = review.work_id;
    const attemptId = review.attempt_id;
    const paths = orderedPaths;
    loadViewed(review.evidence_id);
    void loadStack(paths, workId, attemptId);
  });

  $effect(() => {
    if (typeof localStorage === "undefined") return;
    const saved = localStorage.getItem(densityStorageKey());
    if (saved === "compact" || saved === "comfortable") density = saved;
  });

  async function restorePath(path: string) {
    const comparison = fileDiffs.find((file) => file.path === path);
    if (!comparison) return;
    await onRestore(comparison);
  }

  function toggleViewed(path: string) {
    const next = viewedPaths.includes(path)
      ? viewedPaths.filter((entry) => entry !== path)
      : [...viewedPaths, path];
    viewedPaths = next;
    persistViewed(review.evidence_id, next);
  }

  function toggleCollapsed(path: string) {
    collapsedPaths = collapsedPaths.includes(path)
      ? collapsedPaths.filter((entry) => entry !== path)
      : [...collapsedPaths, path];
  }

  function setDensity(next: "comfortable" | "compact") {
    density = next;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(densityStorageKey(), next);
    }
  }

  function focusFile(index: number) {
    if (!orderedPaths.length) return;
    const clamped = ((index % orderedPaths.length) + orderedPaths.length) % orderedPaths.length;
    focusIndex = clamped;
    const path = orderedPaths[clamped]!;
    const el = document.getElementById(`diff-file-${encodeURIComponent(path)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function onReviewKeydown(event: KeyboardEvent) {
    if (modalBlocksHotkeys?.() || isEditableTarget?.(event.target)) return;
    if (!reviewRoot?.contains(event.target as Node) && document.activeElement !== reviewRoot) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "n") {
      event.preventDefault();
      focusFile(focusIndex + 1);
    } else if (key === "p") {
      event.preventDefault();
      focusFile(focusIndex - 1);
    } else if (key === "j") {
      event.preventDefault();
      focusFile(focusIndex + 1);
    } else if (key === "k") {
      event.preventDefault();
      focusFile(focusIndex - 1);
    } else if (key === "v") {
      event.preventDefault();
      const path = orderedPaths[focusIndex];
      if (path) toggleViewed(path);
    } else if (key === "." && onComment) {
      event.preventDefault();
      const path = orderedPaths[focusIndex];
      if (!path) return;
      onComment({ path, side: "new", line: 1, content: "" });
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="review-surface"
  role="application"
  aria-label="Change review"
  tabindex="0"
  bind:this={reviewRoot}
  onkeydown={onReviewKeydown}
>
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
      <p class="review-summary-kicker">
        {#if review.synthesis.status === "needs_attention"}
          Needs attention
        {:else if review.synthesis.status === "ready"}
          Ready for review
        {:else}
          Ready for review
        {/if}
      </p>
      <p class="review-summary-text">{review.synthesis.status_summary}</p>
      {#if orderedPaths.length > 0}
        <p class="review-progress">{viewedCount} of {orderedPaths.length} files viewed</p>
      {/if}
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
          {actor.label}{#if (actor.count ?? 1) > 1}<span> · {actor.count}</span>{/if}
          {#if actor.files.length > 0}
            <span>· {actor.files.length}</span>
          {/if}
        </span>
      {/each}
    </div>
  {/if}

  {#if review.evidence_id && review.changed_files.length === 0}
    <div class="review-empty-seal" role="status">
      <p>No file changes in this revision</p>
      <span>
        Nothing was sealed — either the attempt wrote no files, everything was reverted, or a different attempt should be selected.
      </span>
      {#if review.candidates.length > 1}
        <label class="review-candidate-picker review-candidate-picker--inline">
          <span>Switch attempt</span>
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
    </div>
  {:else if loading && fileDiffs.length === 0}
    <div class="review-loading">
      <LoaderCircle size={14} class="animate-spin" />
      <span>Preparing comparisons…</span>
    </div>
  {:else}
    {#each Object.entries(fileErrors) as [path, message] (path)}
      <div class="review-file-error">
        <span>{path}: {message}</span>
        <button
          type="button"
          disabled={busy}
          onclick={() => void loadFile(path, review.work_id, review.attempt_id)}
        >Retry</button>
      </div>
    {/each}
    <DiffStack
      files={stackFiles}
      bind:mode
      bind:density
      busy={busy || loading}
      riskPaths={riskPaths}
      viewedPaths={viewedPaths}
      collapsedPaths={collapsedPaths}
      subtitle={review.synthesis.recommended_next_action || undefined}
      onOpenFile={(path, line) => void onOpenFile(path, line)}
      onRestoreFile={(path) => void restorePath(path)}
      onToggleViewed={toggleViewed}
      onToggleCollapsed={toggleCollapsed}
      onDensityChange={setDensity}
      {onComment}
    />
  {/if}
</div>

<style>
  .review-surface {
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .review-candidate-picker {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.65rem;
    color: rgb(var(--theme-text-quiet));
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
    color: rgb(var(--theme-text-quiet));
  }

  .review-progress {
    margin-top: 0.35rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-faint));
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
    color: rgb(var(--theme-text-secondary));
  }

  .review-signal--high {
    border-color: rgb(var(--color-error-500) / 0.35);
    color: rgb(var(--theme-error));
  }

  .review-signal--attention {
    border-color: rgb(var(--color-warning-500) / 0.35);
    color: rgb(var(--theme-warning));
  }

  .review-signal--passed {
    border-color: rgb(var(--color-success-500) / 0.35);
    color: rgb(var(--theme-success));
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
    color: rgb(var(--theme-text-tertiary));
  }

  .review-attribution-chip--human {
    color: rgb(var(--theme-link));
  }

  .review-attribution-chip--agent {
    color: rgb(var(--color-secondary-400, var(--theme-link)));
  }

  .review-attribution-chip--terminal {
    color: rgb(var(--theme-success));
  }

  .review-loading,
  .review-error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.35rem;
    padding: 1.25rem 0.5rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
  }

  .review-file-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.28);
    border-radius: 0.45rem;
    padding: 0.4rem 0.55rem;
    color: rgb(var(--theme-warning));
    font-size: 0.625rem;
  }

  .review-file-error button {
    border: 0;
    border-radius: 0.3rem;
    background: rgb(var(--color-warning-500) / 0.12);
    padding: 0.2rem 0.45rem;
    color: inherit;
    font-size: 0.5625rem;
  }

  .review-empty-seal {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.35rem;
    margin-bottom: 0.75rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.28);
    border-radius: 0.65rem;
    background: rgb(var(--color-warning-500) / 0.06);
    padding: 0.85rem 0.9rem;
  }

  .review-empty-seal p {
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--theme-warning));
  }

  .review-empty-seal > span {
    font-size: 0.6875rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .review-candidate-picker--inline {
    margin: 0.35rem 0 0;
    width: 100%;
  }
</style>
