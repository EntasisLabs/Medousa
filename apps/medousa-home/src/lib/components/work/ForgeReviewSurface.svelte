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
    onToggleCommentRail?: () => void;
  }

  let {
    review,
    busy = false,
    onOpenFile,
    onRestore,
    onSelectCandidate,
    onComment,
    onToggleCommentRail,
  }: Props = $props();

  let mode = $state<"inline" | "side">("inline");
  let density = $state<"comfortable" | "compact">("comfortable");
  let wrap = $state(false);
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

  const stackStats = $derived.by(() => {
    let additions = 0;
    let deletions = 0;
    for (const file of stackFiles) {
      additions += file.additions ?? 0;
      deletions += file.deletions ?? 0;
    }
    return { additions, deletions };
  });

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

  function wrapStorageKey(): string {
    return "medousa-review-wrap";
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
    focusIndex = 0;
    // Multi-file: inventory collapsed except the focused file.
    collapsedPaths = paths.length > 1 ? paths.slice(1) : [];
    await Promise.all(
      paths.map(async (path) => {
        await loadFile(path, workId, attemptId, token);
      }),
    );
    if (token === loadToken) loading = false;
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
    wrap = localStorage.getItem(wrapStorageKey()) === "1";
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
    const wasCollapsed = collapsedPaths.includes(path);
    collapsedPaths = wasCollapsed
      ? collapsedPaths.filter((entry) => entry !== path)
      : [...collapsedPaths, path];
    if (wasCollapsed) {
      focusIndex = Math.max(0, orderedPaths.indexOf(path));
    }
  }

  function setDensity(next: "comfortable" | "compact") {
    density = next;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(densityStorageKey(), next);
    }
  }

  function setWrap(next: boolean) {
    wrap = next;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(wrapStorageKey(), next ? "1" : "0");
    }
  }

  function focusFile(index: number) {
    if (!orderedPaths.length) return;
    const clamped = ((index % orderedPaths.length) + orderedPaths.length) % orderedPaths.length;
    focusIndex = clamped;
    const path = orderedPaths[clamped]!;
    collapsedPaths = collapsedPaths.filter((entry) => entry !== path);
    const el = document.getElementById(`diff-file-${encodeURIComponent(path)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function statusKicker(): string {
    if (review.synthesis.status === "needs_attention") return "Needs attention";
    return "Ready for review";
  }

  function onReviewKeydown(event: KeyboardEvent) {
    if (modalBlocksHotkeys?.() || isEditableTarget?.(event.target)) return;
    if (!reviewRoot?.contains(event.target as Node) && document.activeElement !== reviewRoot) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "n" || key === "j") {
      event.preventDefault();
      focusFile(focusIndex + 1);
    } else if (key === "p" || key === "k") {
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
    } else if (key === "c" && onToggleCommentRail) {
      event.preventDefault();
      onToggleCommentRail();
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

  <div class="review-header-quiet">
    <p class="review-header-line" title={review.synthesis.status_summary}>
      <span class="review-header-kicker">{statusKicker()}</span>
      {#if orderedPaths.length > 0}
        <span class="review-header-sep" aria-hidden="true">·</span>
        <span>
          {orderedPaths.length}
          {orderedPaths.length === 1 ? "file" : "files"}
        </span>
        {#if !loading || stackFiles.length > 0}
          <span class="diff-add">+{stackStats.additions}</span>
          <span class="diff-del">−{stackStats.deletions}</span>
        {/if}
        <span class="review-header-sep" aria-hidden="true">·</span>
        <span class="review-header-viewed">{viewedCount}/{orderedPaths.length} viewed</span>
      {/if}
    </p>
    <div class="review-signals" aria-label="Review signals">
      <span
        class="review-signal review-signal--{review.synthesis.risk}"
        title={review.synthesis.risk_summary}
      >{review.synthesis.risk} risk</span>
      {#if review.synthesis.verification?.success}
        <span
          class="review-signal review-signal--passed"
          title={`${review.synthesis.verification.label} · ${review.synthesis.verification.command.join(" ")}`}
        >Checked</span>
      {/if}
    </div>
  </div>

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
      bind:wrap
      chrome="prefs"
      showJumpList={false}
      busy={busy || loading}
      riskPaths={riskPaths}
      viewedPaths={viewedPaths}
      collapsedPaths={collapsedPaths}
      onOpenFile={(path, line) => void onOpenFile(path, line)}
      onRestoreFile={(path) => void restorePath(path)}
      onToggleViewed={toggleViewed}
      onToggleCollapsed={toggleCollapsed}
      onDensityChange={setDensity}
      onWrapChange={setWrap}
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

  .review-header-quiet {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.1rem 0.1rem 0.55rem;
  }

  .review-header-line {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .review-header-kicker {
    font-weight: 600;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-200));
  }

  .review-header-sep {
    color: rgb(var(--theme-text-faint));
  }

  .review-header-viewed {
    color: rgb(var(--theme-text-quiet));
  }

  .diff-add {
    color: rgb(var(--theme-success));
    font-variant-numeric: tabular-nums;
  }

  .diff-del {
    color: rgb(var(--theme-error));
    font-variant-numeric: tabular-nums;
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
    font-size: 0.6875rem;
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

  .review-signal--low {
    border-color: rgb(var(--color-surface-500) / 0.28);
    color: rgb(var(--theme-text-quiet));
  }

  .review-signal--medium {
    border-color: rgb(var(--color-warning-500) / 0.28);
    color: rgb(var(--theme-warning));
  }

  .review-signal--unchecked {
    border-color: rgb(var(--color-warning-500) / 0.28);
    color: rgb(var(--theme-warning));
  }

  .review-signal--passed {
    border-color: rgb(var(--color-success-500) / 0.35);
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
