<script lang="ts">
  import {
    ArrowUpRight,
    Check,
    CircleAlert,
    FileDiff,
    LoaderCircle,
    MessageSquareText,
    ShieldCheck,
    X,
  } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import ForgeReviewSurface from "$lib/components/work/ForgeReviewSurface.svelte";
  import ReviewCommentRail from "$lib/components/work/ReviewCommentRail.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import {
    addReviewComment,
    deleteReviewComment,
    getChangesFile,
    getForgeChanges,
    resolveReviewComment,
    type ChangesFileDiff,
    type ForgeChanges,
    type ReviewProjection,
  } from "$lib/forge";

  interface Props {
    workId: string;
    projectTitle: string;
    phase: string;
    review?: ReviewProjection | null;
    eventRevision?: number;
    onOpenCode: (path?: string, line?: number) => void | Promise<void>;
    onRequestRevision: (prompt?: string) => void;
    onReviewChanged?: () => void | Promise<void>;
  }

  let {
    workId,
    projectTitle,
    phase,
    review = null,
    eventRevision = 0,
    onOpenCode,
    onRequestRevision,
    onReviewChanged,
  }: Props = $props();

  let changes = $state<ForgeChanges | null>(null);
  let fileDiffs = $state<Record<string, ChangesFileDiff>>({});
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let sheetOpen = $state(false);
  let commentCompose = $state<{
    path: string;
    side: "new" | "old";
    line: number;
    content: string;
  } | null>(null);
  let commentDraft = $state("");
  let commentBusy = $state(false);
  let commentError = $state<string | null>(null);
  let requestSerial = 0;

  const sealedReview = $derived(
    review?.work_id === workId &&
      (phase === "review" || review.human_phase === "review")
      ? review
      : null,
  );

  function toStackFile(diff: ChangesFileDiff): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    return {
      path: diff.path,
      oldPath: diff.old_path,
      status: diff.status,
      binary: diff.binary,
      conflict: diff.conflict,
      additions: stats.additions,
      deletions: stats.deletions,
      hunks: diff.hunks,
      baselineBytes: diff.baseline.byte_size,
      reviewedBytes: diff.working.byte_size,
      baselineExists: diff.baseline.exists,
      reviewedExists: diff.working.exists,
      beforeText: diff.baseline.content ?? null,
      afterText: diff.working.content ?? null,
    };
  }

  const workingFiles = $derived.by(() =>
    (changes?.files ?? [])
      .map((file) => fileDiffs[file.path])
      .filter((diff): diff is ChangesFileDiff => Boolean(diff))
      .map(toStackFile),
  );

  const receiptFiles = $derived.by(() => {
    if (sealedReview) {
      return sealedReview.changed_files.map((file) => ({
        path: file.path,
        additions: file.lines_added ?? 0,
        deletions: file.lines_removed ?? 0,
      }));
    }
    return workingFiles.map((file) => ({
      path: file.path,
      additions: file.additions ?? 0,
      deletions: file.deletions ?? 0,
    }));
  });

  const expectedFileCount = $derived(
    sealedReview?.changed_files.length ?? changes?.files.length ?? 0,
  );
  const additions = $derived(receiptFiles.reduce((total, file) => total + file.additions, 0));
  const deletions = $derived(receiptFiles.reduce((total, file) => total + file.deletions, 0));
  const visibleFiles = $derived(receiptFiles.slice(0, 3));
  const moreFileCount = $derived(Math.max(0, expectedFileCount - visibleFiles.length));
  const isReady = $derived(Boolean(sealedReview));
  const verification = $derived(sealedReview?.synthesis.verification ?? null);
  const risk = $derived(sealedReview?.synthesis.risk ?? null);

  function basename(path: string): string {
    return path.replaceAll("\\", "/").split("/").at(-1) || path;
  }

  function parentPath(path: string): string {
    const normalized = path.replaceAll("\\", "/");
    const index = normalized.lastIndexOf("/");
    return index > 0 ? normalized.slice(0, index) : "";
  }

  async function loadWorkingChanges(id: string) {
    const serial = ++requestSerial;
    loading = true;
    loadError = null;
    try {
      const snapshot = await getForgeChanges(id);
      if (serial !== requestSerial) return;
      changes = snapshot;
      fileDiffs = {};
      const next: Record<string, ChangesFileDiff> = {};
      let cursor = 0;
      let previewFailed = false;
      const loadNext = async () => {
        while (cursor < snapshot.files.length) {
          const file = snapshot.files[cursor++];
          if (!file) return;
          try {
            const diff = await getChangesFile(id, file.path);
            next[diff.path] = diff;
            if (serial === requestSerial) fileDiffs = { ...next };
          } catch {
            previewFailed = true;
          }
        }
      };
      await Promise.all(
        Array.from(
          { length: Math.min(4, snapshot.files.length) },
          () => loadNext(),
        ),
      );
      if (serial !== requestSerial) return;
      fileDiffs = next;
      if (previewFailed) {
        loadError = "Some file previews could not be loaded.";
      }
    } catch (error) {
      if (serial !== requestSerial) return;
      changes = null;
      fileDiffs = {};
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      if (serial === requestSerial) loading = false;
    }
  }

  function closeSheet() {
    sheetOpen = false;
  }

  function askForRevision() {
    const notes = (sealedReview?.comments ?? [])
      .filter((comment) => !comment.resolved_at)
      .map((comment) => `- ${comment.path}:${comment.start_line} — ${comment.body}`)
      .join("\n");
    closeSheet();
    onRequestRevision(
      notes
        ? `Revise the current changes in ${projectTitle} using these review notes:\n\n${notes}`
        : undefined,
    );
  }

  function openComment(input: {
    path: string;
    side: "new" | "old";
    line: number;
    content: string;
  }) {
    commentCompose = input;
    commentDraft = "";
    commentError = null;
  }

  async function submitComment() {
    if (
      !sealedReview?.evidence_id ||
      !commentCompose ||
      !commentDraft.trim() ||
      commentBusy
    ) return;
    commentBusy = true;
    commentError = null;
    try {
      await addReviewComment(workId, {
        evidence_id: sealedReview.evidence_id,
        attempt_id: sealedReview.attempt_id ?? undefined,
        path: commentCompose.path,
        side: commentCompose.side,
        start_line: commentCompose.line,
        end_line: commentCompose.line,
        anchor_text: commentCompose.content || null,
        body: commentDraft.trim(),
      });
      commentCompose = null;
      commentDraft = "";
      await onReviewChanged?.();
    } catch (error) {
      commentError = error instanceof Error ? error.message : String(error);
    } finally {
      commentBusy = false;
    }
  }

  async function resolveComment(commentId: string) {
    if (commentBusy) return;
    commentBusy = true;
    commentError = null;
    try {
      await resolveReviewComment(workId, commentId);
      await onReviewChanged?.();
    } catch (error) {
      commentError = error instanceof Error ? error.message : String(error);
    } finally {
      commentBusy = false;
    }
  }

  async function removeComment(commentId: string) {
    if (commentBusy) return;
    commentBusy = true;
    commentError = null;
    try {
      await deleteReviewComment(workId, commentId);
      await onReviewChanged?.();
    } catch (error) {
      commentError = error instanceof Error ? error.message : String(error);
    } finally {
      commentBusy = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!sheetOpen || event.key !== "Escape") return;
    event.preventDefault();
    closeSheet();
  }

  $effect(() => {
    const id = workId.trim();
    void eventRevision;
    if (!id || sealedReview) return;
    const timer = window.setTimeout(() => void loadWorkingChanges(id), 220);
    return () => window.clearTimeout(timer);
  });

  $effect(() => {
    if (!sheetOpen) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if expectedFileCount > 0}
  <section class="chat-change-receipt" aria-label={isReady ? "Ready for review" : "Working changes"}>
    <header class="chat-change-receipt-header">
      <span class="chat-change-receipt-icon" class:chat-change-receipt-icon--ready={isReady}>
        {#if isReady}
          <ShieldCheck size={15} strokeWidth={1.8} />
        {:else}
          <FileDiff size={15} strokeWidth={1.8} />
        {/if}
      </span>
      <div class="chat-change-receipt-heading">
        <p>{isReady ? "Ready for review" : "Working changes"}</p>
        <span>{expectedFileCount} {expectedFileCount === 1 ? "file" : "files"}</span>
      </div>
      <div
        class="chat-change-receipt-stats"
        aria-label={`${additions} additions and ${deletions} deletions${loading && !isReady ? " counted so far" : ""}`}
      >
        {#if loading && !isReady}<LoaderCircle size={10} class="animate-spin" />{/if}
        <span class="chat-change-add">+{additions}</span>
        <span class="chat-change-del">−{deletions}</span>
      </div>
      <button type="button" class="chat-change-review-button" onclick={() => (sheetOpen = true)}>
        Review
      </button>
    </header>

    {#if isReady && (verification || risk)}
      <div class="chat-change-signals">
        {#if verification}
          <span class:chat-change-signal--success={verification.success}>
            {#if verification.success}<Check size={11} />{:else}<CircleAlert size={11} />{/if}
            {verification.success ? "Checks passed" : "Checks need attention"}
          </span>
        {/if}
        {#if risk}
          <span class:chat-change-signal--attention={risk !== "low"}>{risk} risk</span>
        {/if}
      </div>
    {/if}

    <ul class="chat-change-files">
      {#each visibleFiles as file (file.path)}
        <li>
          <span class="chat-change-file-name">{basename(file.path)}</span>
          {#if parentPath(file.path)}
            <span class="chat-change-file-parent">{parentPath(file.path)}</span>
          {/if}
          <span class="chat-change-file-stats">
            <span class="chat-change-add">+{file.additions}</span>
            <span class="chat-change-del">−{file.deletions}</span>
          </span>
        </li>
      {/each}
    </ul>

    {#if moreFileCount > 0 || loadError}
      <footer class="chat-change-receipt-footer">
        {#if moreFileCount > 0}
          <button type="button" onclick={() => (sheetOpen = true)}>
            Show {moreFileCount} more {moreFileCount === 1 ? "file" : "files"}
          </button>
        {/if}
        {#if loadError}<span title={loadError}>Preview incomplete</span>{/if}
      </footer>
    {/if}
  </section>
{:else if loading}
  <div class="chat-change-loading" aria-label="Loading working changes">
    <LoaderCircle size={12} class="animate-spin" />
    Checking the working copy…
  </div>
{/if}

{#if sheetOpen}
  <BodyPortal>
    <div
      class="chat-review-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={`Review changes in ${projectTitle}`}
      tabindex="-1"
      onclick={(event) => {
        if (event.target === event.currentTarget) closeSheet();
      }}
      onkeydown={handleKeydown}
    >
      <section class="chat-review-sheet">
        <header class="chat-review-chrome">
          <div class="chat-review-title">
            <span>{isReady ? "Ready for review" : "Working changes"}</span>
            <h2>{projectTitle}</h2>
          </div>
          <div class="chat-review-summary">
            <span>{expectedFileCount} {expectedFileCount === 1 ? "file" : "files"}</span>
            <span class="chat-change-add">+{additions}</span>
            <span class="chat-change-del">−{deletions}</span>
          </div>
          <button type="button" class="chat-review-close" aria-label="Close review" onclick={closeSheet}>
            <X size={17} strokeWidth={1.8} />
          </button>
        </header>

        {#if isReady && sealedReview}
          <div class="chat-review-signal-bar">
            {#if verification}
              <span class:chat-change-signal--success={verification.success}>
                {#if verification.success}<Check size={12} />{:else}<CircleAlert size={12} />{/if}
                {verification.label || (verification.success ? "Checks passed" : "Checks failed")}
              </span>
            {/if}
            <span class:chat-change-signal--attention={risk !== "low"}>
              {sealedReview.synthesis.risk_summary || `${risk ?? "unknown"} risk`}
            </span>
          </div>
        {/if}

        <div class="chat-review-content">
          <div class="chat-review-body">
            {#if isReady && sealedReview}
              <ForgeReviewSurface
                review={sealedReview}
                {projectTitle}
                onOpenFile={(path, line) => onOpenCode(path, line)}
                onComment={openComment}
              />
            {:else if loading && workingFiles.length === 0}
              <div class="chat-review-empty"><LoaderCircle size={16} class="animate-spin" />Loading changes…</div>
            {:else if workingFiles.length > 0}
              <DiffStack
                files={workingFiles}
                density="compact"
                chrome="prefs"
                showJumpList
                wrap={layout.isMobile}
                onOpenFile={(path, line) => onOpenCode(path, line)}
              />
            {:else}
              <div class="chat-review-empty">No text changes to preview.</div>
            {/if}
          </div>
          {#if isReady && sealedReview && (commentCompose || (sealedReview.comments?.length ?? 0) > 0)}
            <div class="chat-review-comments">
              <ReviewCommentRail
                comments={sealedReview.comments ?? []}
                compose={commentCompose}
                draft={commentDraft}
                busy={commentBusy}
                onDraftChange={(value) => (commentDraft = value)}
                onSubmit={submitComment}
                onCancelCompose={() => {
                  commentCompose = null;
                  commentDraft = "";
                }}
                onResolve={resolveComment}
                onDelete={removeComment}
                onJump={(comment) => {
                  document
                    .getElementById(`diff-file-${encodeURIComponent(comment.path)}`)
                    ?.scrollIntoView({ behavior: "smooth", block: "start" });
                }}
              />
              {#if commentError}<p class="chat-review-comment-error">{commentError}</p>{/if}
            </div>
          {/if}
        </div>

        <footer class="chat-review-actions">
          <button type="button" class="chat-review-action chat-review-action--quiet" onclick={() => void onOpenCode()}>
            <ArrowUpRight size={13} />
            Open in Code
          </button>
          <button type="button" class="chat-review-action chat-review-action--primary" onclick={askForRevision}>
            <MessageSquareText size={13} />
            Ask Medousa to revise
          </button>
        </footer>
      </section>
    </div>
  </BodyPortal>
{/if}

<style>
  .chat-change-receipt {
    width: min(46rem, calc(100% - 1rem));
    margin: 0.35rem auto 0;
    overflow: hidden;
    border: 1px solid rgb(var(--theme-border) / 0.24);
    border-radius: var(--theme-container-radius);
    background: rgb(var(--theme-card) / 0.72);
    box-shadow: 0 0.4rem 1.25rem rgb(var(--theme-shadow) / 0.08);
    color: rgb(var(--theme-text));
  }

  .chat-change-receipt-header {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.65rem;
    padding: 0.7rem 0.75rem 0.55rem;
  }

  .chat-change-receipt-icon {
    display: grid;
    width: 1.9rem;
    height: 1.9rem;
    flex: 0 0 auto;
    place-items: center;
    border-radius: var(--theme-control-radius);
    background: rgb(var(--theme-pane-muted) / 0.72);
    color: rgb(var(--theme-text-secondary));
  }

  .chat-change-receipt-icon--ready {
    background: rgb(var(--theme-success) / 0.12);
    color: rgb(var(--theme-success));
  }

  .chat-change-receipt-heading {
    min-width: 0;
    flex: 1 1 auto;
  }

  .chat-change-receipt-heading p {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .chat-change-receipt-heading span,
  .chat-change-receipt-footer,
  .chat-change-loading {
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.625rem;
  }

  .chat-change-receipt-stats,
  .chat-change-file-stats,
  .chat-review-summary {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.625rem;
    font-variant-numeric: tabular-nums;
  }

  .chat-change-add {
    color: rgb(var(--syn-addition-fg));
  }

  .chat-change-del {
    color: rgb(var(--syn-deletion-fg));
  }

  .chat-change-review-button,
  .chat-review-action {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    border-radius: var(--theme-control-radius);
    font-size: 0.6875rem;
    font-weight: 550;
  }

  .chat-change-review-button {
    border: 1px solid rgb(var(--theme-border) / 0.32);
    padding: 0.32rem 0.6rem;
    background: rgb(var(--theme-pane-muted) / 0.55);
    color: rgb(var(--theme-text));
  }

  .chat-change-review-button:hover,
  .chat-change-review-button:focus-visible {
    border-color: rgb(var(--theme-focus) / 0.48);
    background: rgb(var(--theme-card-hover) / 0.72);
  }

  .chat-change-signals,
  .chat-review-signal-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem 0.8rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.625rem;
  }

  .chat-change-signals {
    border-top: 1px solid rgb(var(--theme-border) / 0.14);
    padding: 0.42rem 0.75rem;
  }

  .chat-change-signals span,
  .chat-review-signal-bar span {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }

  .chat-change-signal--success {
    color: rgb(var(--theme-success));
  }

  .chat-change-signal--attention {
    color: rgb(var(--theme-warning));
  }

  .chat-change-files {
    margin: 0;
    padding: 0;
    border-top: 1px solid rgb(var(--theme-border) / 0.14);
    list-style: none;
  }

  .chat-change-files li {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 0.45rem;
    padding: 0.36rem 0.75rem;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.1);
  }

  .chat-change-files li:last-child {
    border-bottom: 0;
  }

  .chat-change-file-name {
    flex: 0 1 auto;
    overflow: hidden;
    color: rgb(var(--theme-text-secondary));
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chat-change-file-parent {
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chat-change-file-stats {
    margin-left: auto;
  }

  .chat-change-receipt-footer {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    border-top: 1px solid rgb(var(--theme-border) / 0.14);
    padding: 0.4rem 0.75rem;
  }

  .chat-change-receipt-footer button:hover,
  .chat-change-receipt-footer button:focus-visible {
    color: rgb(var(--theme-link));
  }

  .chat-change-loading {
    display: flex;
    width: min(46rem, calc(100% - 1rem));
    margin: 0.35rem auto 0;
    align-items: center;
    gap: 0.4rem;
    padding: 0.45rem 0.65rem;
  }

  .chat-review-backdrop {
    position: fixed;
    inset: 0;
    z-index: 125;
    display: flex;
    justify-content: flex-end;
    background: rgb(var(--theme-shadow) / 0.42);
    animation: chat-review-fade-in 150ms ease-out;
  }

  .chat-review-sheet {
    display: flex;
    box-sizing: border-box;
    width: min(54rem, calc(100vw - 4rem));
    min-width: 0;
    height: 100%;
    flex-direction: column;
    overflow: hidden;
    border-left: 1px solid rgb(var(--theme-border) / 0.3);
    background: rgb(var(--theme-pane));
    box-shadow: -1rem 0 3rem rgb(var(--theme-shadow) / 0.24);
    color: rgb(var(--theme-text));
    animation: chat-review-slide-in 220ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .chat-review-chrome {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.8rem;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.2);
    padding: 0.7rem 0.85rem;
  }

  .chat-review-title {
    min-width: 0;
    flex: 1 1 auto;
  }

  .chat-review-title span {
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.625rem;
  }

  .chat-review-title h2 {
    margin: 0.08rem 0 0;
    overflow: hidden;
    font-size: 0.8125rem;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chat-review-summary {
    color: rgb(var(--theme-text-tertiary));
  }

  .chat-review-close {
    display: grid;
    width: 1.8rem;
    height: 1.8rem;
    flex: 0 0 auto;
    place-items: center;
    border-radius: var(--theme-control-radius);
    color: rgb(var(--theme-text-tertiary));
  }

  .chat-review-close:hover,
  .chat-review-close:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.65);
    color: rgb(var(--theme-text));
  }

  .chat-review-signal-bar {
    border-bottom: 1px solid rgb(var(--theme-border) / 0.16);
    padding: 0.5rem 0.9rem;
  }

  .chat-review-content {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1 1 auto;
    overflow: hidden;
  }

  .chat-review-body {
    min-width: 0;
    min-height: 0;
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 0.9rem;
  }

  .chat-review-comments {
    width: min(19rem, 34%);
    flex: 0 0 auto;
    overflow-y: auto;
    border-left: 1px solid rgb(var(--theme-border) / 0.16);
    padding: 0.75rem;
  }

  .chat-review-comments :global(.review-comment-rail) {
    min-width: 0;
    max-width: none;
    border-left: 0;
    padding-left: 0;
  }

  .chat-review-comment-error {
    margin: 0.5rem 0 0;
    color: rgb(var(--theme-error));
    font-size: 0.625rem;
  }

  .chat-review-empty {
    display: flex;
    min-height: 12rem;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
  }

  .chat-review-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    border-top: 1px solid rgb(var(--theme-border) / 0.2);
    padding: 0.65rem 0.85rem max(0.65rem, env(safe-area-inset-bottom, 0px));
  }

  .chat-review-action {
    padding: 0.42rem 0.7rem;
  }

  .chat-review-action--quiet {
    color: rgb(var(--theme-text-secondary));
  }

  .chat-review-action--quiet:hover,
  .chat-review-action--quiet:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.6);
    color: rgb(var(--theme-text));
  }

  .chat-review-action--primary {
    background: rgb(var(--theme-action));
    color: rgb(var(--on-primary));
  }

  .chat-review-action--primary:hover,
  .chat-review-action--primary:focus-visible {
    filter: brightness(1.06);
  }

  @media (max-width: 48rem) {
    .chat-review-sheet {
      width: 100%;
      height: var(--mobile-layout-height, 100dvh);
      border-left: 0;
      padding-top: env(safe-area-inset-top, 0px);
    }

    .chat-change-file-parent {
      display: none;
    }

    .chat-review-summary {
      display: none;
    }

    .chat-review-content {
      flex-direction: column;
      overflow-y: auto;
    }

    .chat-review-body {
      flex: 0 0 auto;
      overflow: visible;
      padding: 0.75rem;
    }

    .chat-review-comments {
      width: auto;
      overflow: visible;
      border-top: 1px solid rgb(var(--theme-border) / 0.16);
      border-left: 0;
    }
  }

  @keyframes chat-review-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes chat-review-slide-in {
    from { transform: translateX(1.5rem); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>
