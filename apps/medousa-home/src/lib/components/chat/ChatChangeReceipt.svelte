<script lang="ts">
  import "$lib/styles/chat-review.postcss";
  import MobileActionSheet from "$lib/components/mobile/MobileActionSheet.svelte";
  import ReviewGitActions from "./ReviewGitActions.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { readReviewNotes, writeReviewNotes, reviewNotesKey, reviewNotesPrompt, type WorkingReviewNote, type WorkingReviewAnchor } from "$lib/chat/workingReviewNotes";
  import { untrack } from "svelte";
  import { subscribeCodeProjectEvents } from "$lib/code/codeProjectEvents";
  import {
    ArrowUpRight,
    Check,
    CircleAlert,
    FileDiff,
    LoaderCircle,
    MessageSquareText,
    RefreshCw,
    ShieldCheck,
    X,
  } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import WorkingChangesBrowser from "$lib/components/chat/WorkingChangesBrowser.svelte";
  import ForgeReviewSurface from "$lib/components/work/ForgeReviewSurface.svelte";
  import ReviewCommentRail from "$lib/components/work/ReviewCommentRail.svelte";
  import {
    addReviewComment,
    deleteReviewComment,
    getForgeChanges,
    resolveReviewComment,
    type ForgeChanges,
    type ReviewProjection,
  } from "$lib/forge";

  interface Props {
    workId: string;
    projectTitle: string;
    phase: string;
    review?: ReviewProjection | null;
    eventRevision?: number;
    activityRunning?: boolean;
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
    activityRunning = false,
    onOpenCode,
    onRequestRevision,
    onReviewChanged,
  }: Props = $props();

  let page = $state<"review" | "comment" | "notes" | "commit" | "pull-request">("review");
  let gitBusy = $state(false);
  let noteAnchor = $state<WorkingReviewAnchor | null>(null);
  let notes = $state<WorkingReviewNote[]>([]);
  let notesKey = $state("");
  let selectedPath = $state<string | null>(null);
  const sheetTitle = $derived({ review: "Changes", comment: "Add comment", notes: "Review notes", commit: "Commit changes", "pull-request": "Create pull request" }[page]);
  $effect(() => {
    const key = reviewNotesKey(workId);
    notesKey = key; notes = readReviewNotes(key); sheetOpen = false; page = "review"; selectedPath = null;
  });
  function back() { if (page !== "review") page = "review"; else selectedPath = null; }
  function openWorkingComment(anchor: WorkingReviewAnchor) {
    noteAnchor = anchor; commentCompose = null; commentDraft = ""; commentError = null; page = "comment";
  }
  function saveNote() {
    if (!noteAnchor || !commentDraft.trim()) return;
    const next = [...notes, { ...noteAnchor, id: crypto.randomUUID(), body: commentDraft.trim() }];
    try { writeReviewNotes(notesKey, next); notes = next; page = "review"; noteAnchor = null; }
    catch { commentError = "Could not save this note on your device. Your comment is still here."; }
  }
  function removeNote(id: string) {
    const next = notes.filter((note) => note.id !== id);
    try { writeReviewNotes(notesKey, next); notes = next; } catch { commentError = "Could not remove this note."; }
  }

  let changes = $state<ForgeChanges | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let sheetOpen = $state(false);
  let refreshAvailable = $state(false);
  let projectRevision = $state(0);

  $effect(() => {
    const id = workId.trim();
    if (!id) return;
    const invalidate = () => { projectRevision += 1; };
    return subscribeCodeProjectEvents(id, { onEvent: invalidate, onResync: invalidate });
  });

  $effect(() => {
    if (!activityRunning) untrack(() => { projectRevision += 1; });
  });
  let loadedEventRevision = $state(-1);
  let commentCompose = $state<{
    path: string;
    side: "new" | "old";
    line: number;
    content: string;
  } | null>(null);
  let commentDraft = $state("");
  let commentBusy = $state(false);
  let commentError = $state<string | null>(null);
  let snapshotRequestSerial = 0;
  let loadedWorkId = "";

  const sealedReview = $derived(
    review?.work_id === workId &&
      (phase === "review" || review.human_phase === "review")
      ? review
      : null,
  );

  const receiptFiles = $derived.by(() => {
    if (sealedReview) {
      return sealedReview.changed_files.map((file) => ({
        path: file.path,
        additions: file.lines_added ?? 0,
        deletions: file.lines_removed ?? 0,
        status: file.status,
      }));
    }
    return (changes?.files ?? []).map((file) => ({
      path: file.path,
      additions: null,
      deletions: null,
      status: file.status,
    }));
  });

  const expectedFileCount = $derived(
    sealedReview?.changed_files.length ?? changes?.files.length ?? 0,
  );
  const additions = $derived(
    receiptFiles.reduce((total, file) => total + (file.additions ?? 0), 0),
  );
  const deletions = $derived(
    receiptFiles.reduce((total, file) => total + (file.deletions ?? 0), 0),
  );
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

  function statusLabel(status: string): string {
    switch (status) {
      case "added": return "Added";
      case "deleted": return "Deleted";
      case "renamed": return "Renamed";
      case "copied": return "Copied";
      case "type_changed": return "Type changed";
      case "untracked": return "New";
      case "unmerged": return "Conflict";
      default: return "Modified";
    }
  }

  async function loadWorkingChanges(id: string, revision: number) {
    const serial = ++snapshotRequestSerial;
    loading = true;
    loadError = null;
    try {
      const snapshot = await getForgeChanges(id);
      if (serial !== snapshotRequestSerial) return;
      changes = snapshot;
      loadedWorkId = id;
      loadedEventRevision = revision;
      refreshAvailable = false;
    } catch (error) {
      if (serial !== snapshotRequestSerial) return;
      changes = null;
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      if (serial === snapshotRequestSerial) loading = false;
    }
  }

  async function refreshWorkingSnapshot() {
    const id = workId.trim();
    if (!id) return;
    await loadWorkingChanges(id, eventRevision + projectRevision);
  }

  function closeSheet() {
    if (gitBusy || commentBusy) return;
    sheetOpen = false;
    page = "review";
  }

  function askForRevision() {
    const sealedNotes = (sealedReview?.comments ?? [])
      .filter((comment) => !comment.resolved_at)
      .map((comment) => `- ${comment.path}:${comment.start_line} — ${comment.body}`)
      .join("\n");
    closeSheet();
    onRequestRevision(
      notes.length ? reviewNotesPrompt(projectTitle, notes) + (sealedNotes ? `\n\nAdditional review notes:\n${sealedNotes}` : "") : sealedNotes
        ? `Revise the current changes in ${projectTitle} using these review notes:\n\n${sealedNotes}`
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
    noteAnchor = null;
    page = "comment";
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
      page = "review";
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
    const revision = eventRevision + projectRevision;
    const reviewReady = Boolean(sealedReview);
    if (!id || reviewReady) {
      snapshotRequestSerial += 1;
      changes = null;
      loadedWorkId = "";
      loadedEventRevision = -1;
      refreshAvailable = false;
      return;
    }
    if (sheetOpen && loadedWorkId === id && loadedEventRevision >= 0) {
      if (revision !== loadedEventRevision) refreshAvailable = true;
      return;
    }
    const timer = window.setTimeout(() => void loadWorkingChanges(id, revision), 220);
    return () => {
      window.clearTimeout(timer);
      snapshotRequestSerial += 1;
      loading = false;
    };
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
  <section
    class="chat-change-receipt"
    class:chat-change-receipt--ready={isReady}
    aria-label={isReady ? "Ready for review" : "Working changes"}
  >
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
      {#if isReady}
        <div
          class="chat-change-receipt-stats"
          aria-label={`${additions} additions and ${deletions} deletions`}
        >
          <span class="chat-change-add">+{additions}</span>
          <span class="chat-change-del">−{deletions}</span>
        </div>
      {:else if loading}
        <LoaderCircle size={11} class="animate-spin chat-change-refreshing" aria-label="Refreshing changes" />
      {/if}
      <button
        type="button"
        class="chat-change-review-button"
        aria-label={isReady ? "Review changes" : "View working changes"}
        onclick={() => (sheetOpen = true)}
      >
        {isReady ? "Review" : "View"}
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
          {#if file.additions != null && file.deletions != null}
            <span class="chat-change-file-stats">
              <span class="chat-change-add">+{file.additions}</span>
              <span class="chat-change-del">−{file.deletions}</span>
            </span>
          {:else if file.status}
            <span class="chat-change-file-status">{statusLabel(file.status)}</span>
          {/if}
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

{#if sheetOpen && layout.isMobile}
  <MobileActionSheet bind:open={sheetOpen} title={sheetTitle} footer={page === "review" ? actions : undefined} full busy={gitBusy || commentBusy} onclose={closeSheet}
    onback={page !== "review" || selectedPath ? back : undefined}>
    {#if refreshAvailable && page === "review"}
      <div class="chat-review-update-bar"><span>Changes updated while you were reviewing.</span><button type="button" onclick={() => void refreshWorkingSnapshot()} disabled={loading}>Refresh</button></div>
    {/if}
    {@render body()}
  </MobileActionSheet>
{:else if sheetOpen}
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
            <span>{page !== "review" ? sheetTitle : isReady ? "Ready for review" : "Working changes"}</span>
            <h2>{projectTitle}</h2>
          </div>
          <div class="chat-review-summary">
            <span>{expectedFileCount} {expectedFileCount === 1 ? "file" : "files"}</span>
            {#if isReady}
              <span class="chat-change-add">+{additions}</span>
              <span class="chat-change-del">−{deletions}</span>
            {/if}
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
        {:else if refreshAvailable}
          <div class="chat-review-update-bar">
            <span>Changes updated while you were reviewing.</span>
            <button type="button" onclick={() => void refreshWorkingSnapshot()} disabled={loading}>
              <RefreshCw size={12} class={loading ? "animate-spin" : undefined} />
              Refresh
            </button>
          </div>
        {/if}

        {#if page === "review"}
          {@render body()}
          {@render actions()}
        {:else}
          <div class="chat-review-page">{@render body()}</div>
          <footer class="chat-review-actions"><button type="button" class="chat-review-action" disabled={gitBusy || commentBusy} onclick={back}>Back to changes</button></footer>
        {/if}
      </section>
    </div>
  </BodyPortal>
{/if}



{#snippet actions()}
<footer class="chat-review-actions">
  <button type="button" class="chat-review-action" onclick={() => page = "notes"}>Notes ({notes.length + (sealedReview?.comments?.length ?? 0)})</button>
  <button type="button" class="chat-review-action" onclick={() => page = "commit"}>Commit</button>
  <button type="button" class="chat-review-action chat-review-action--primary" onclick={() => page = "pull-request"}>Create PR</button>
</footer>
{/snippet}
{#snippet body()}
  {#if page === "commit" || page === "pull-request"}
    <ReviewGitActions {workId} action={page} onbusy={(value) => gitBusy = value} ondone={() => { page = "review"; void refreshWorkingSnapshot(); void onReviewChanged?.(); }}/>
  {:else if page === "comment"}
    <form class="review-note-form" onsubmit={(event) => { event.preventDefault(); if (noteAnchor) saveNote(); else void submitComment(); }}>
      <p>{noteAnchor?.path ?? commentCompose?.path}:{noteAnchor?.line ?? commentCompose?.line}</p>
      <pre>{noteAnchor?.content ?? commentCompose?.content}</pre>
      <label>Comment<textarea class="textarea" rows="5" bind:value={commentDraft} disabled={commentBusy} placeholder="What should change?"></textarea></label>
      {#if noteAnchor}<p class="review-note-hint">Saved on this device with the version you reviewed. Send your notes to Medousa when ready.</p>{/if}
      {#if commentError}<p role="alert">{commentError}</p>{/if}
      <button type="submit" class="chat-review-action chat-review-action--primary" disabled={commentBusy || !commentDraft.trim()}>Save comment</button>
    </form>
  {:else if page === "notes"}
    <div class="review-notes">
      {#each notes as note (note.id)}
        <article><strong>{note.path}:{note.line}</strong><p>{note.body}</p><small>Attached to the captured {note.side === "new" ? "working" : "original"} version</small><button type="button" onclick={() => removeNote(note.id)}>Remove note</button></article>
      {/each}
      {#if sealedReview}
        <ReviewCommentRail comments={sealedReview.comments ?? []} compose={null} draft="" busy={commentBusy}
          onDraftChange={() => {}} onSubmit={submitComment} onCancelCompose={() => {}}
          onResolve={resolveComment} onDelete={removeComment} onJump={() => page = "review"}/>
      {/if}
      {#if !notes.length && !sealedReview?.comments?.length}<p>Select a line in the diff to leave a comment.</p>{/if}
      {#if commentError}<p role="alert">{commentError}</p>{/if}
      <button type="button" class="chat-review-action chat-review-action--primary" onclick={askForRevision}>Ask Medousa to revise</button>
    </div>
  {:else}
        <div class="chat-review-content">
          <div class="chat-review-body">
            {#if isReady && sealedReview}
              <ForgeReviewSurface
                review={sealedReview}
                {projectTitle}
                onOpenFile={(path, line) => onOpenCode(path, line)}
                onComment={openComment}
              />
            {:else}
              {#key `${workId}:${loadedEventRevision}`}
                <WorkingChangesBrowser
                  {workId}
                  bind:selectedPath
                  onComment={openWorkingComment}
                  {changes}
                  {loading}
                  error={loadError}
                  onOpenFile={(path, line) => onOpenCode(path, line)}
                  onRefresh={refreshWorkingSnapshot}
                />
              {/key}
            {/if}
          </div>
          {#if !layout.isMobile && isReady && sealedReview && (commentCompose || (sealedReview.comments?.length ?? 0) > 0)}
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

  {/if}
{/snippet}
