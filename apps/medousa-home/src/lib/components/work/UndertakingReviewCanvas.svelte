<script lang="ts">
  import "$lib/styles/review-canvas.postcss";
  import { tick, untrack } from "svelte";
  import { changeKindLabels, type ReviewChangeKind } from "$lib/code/reviewChangeKinds";
  import { reviewAreas } from "$lib/code/reviewAreas";
  import ReviewPageHeading from "./ReviewPageHeading.svelte";
  import ReviewOverview from "$lib/components/work/ReviewOverview.svelte";
  import type { ReviewReadingState, ReviewLens } from "$lib/code/reviewPage.svelte";
  import { ChevronRight, CircleAlert, GitCommitHorizontal, Link2, Save } from "@lucide/svelte";
  import ForgeReviewSurface from "$lib/components/work/ForgeReviewSurface.svelte";
  import ReviewCommentRail from "$lib/components/work/ReviewCommentRail.svelte";
  import ReviewProvenanceStrip from "$lib/components/work/ReviewProvenanceStrip.svelte";
  import {
    type EvidencePage,
    type ProviderHandoff,
    type ReviewFileDiff,
    type ReviewProjection,
  } from "$lib/code/undertakingCommandController";

  interface Props {
    review: ReviewProjection | null;
    readingState: ReviewReadingState;
    originalRequest: string;
    sourceBranch: string;
    humanPhase: string;
    projectTitle: string;
    provenanceBaseRef: string | null;
    busy: boolean;
    acknowledgePolicy: boolean;
    acknowledgeBlocking: boolean;
    requestChangesPreviewOpen: boolean;
    reviewDetailsOpen: boolean;
    reviewAuditOpen: boolean;
    providerLink: string;
    commentDraft: string;
    commentCompose: {
      path: string;
      side: "new" | "old" | string;
      line: number;
      content: string;
    } | null;
    reviewRationale: string;
    exportOpen: boolean;
    exportDestination: string;
    reviewHardBlockingMessages: string[];
    reviewFooterIssues: string[];
    issueLabel: (issue: string) => string;
    showCommentRail: boolean;
    reviewDetailsLoading: boolean;
    providerLinkOpen: boolean;
    providerHandoff: ProviderHandoff | null;
    commands: EvidencePage | null;
    exportedDestination: string | null;
    reviewNoteOpen: boolean;
    reviewAllowed: boolean;
    remoteExport: boolean;
    onOpenFile: (path: string, line?: number) => void;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
    onSelectCandidate: (attemptId: string) => void;
    onComment: (input: {
      path: string;
      side: "new" | "old" | string;
      line: number;
      content: string;
    }) => void;
    onToggleCommentRail: () => void;
    onExport: () => void;
    onAddProviderLink: () => void;
    onLoadMoreCommands: () => void;
    onSubmitComment: () => Promise<void>;
    onCancelCompose: () => void;
    onResolveComment: (commentId: string) => Promise<void>;
    onDeleteComment: (commentId: string) => Promise<void>;
    onConfirmExport: () => void;
  }

  let {
    review,
    readingState,
    originalRequest,
    sourceBranch,
    humanPhase,
    projectTitle,
    provenanceBaseRef,
    busy,
    acknowledgePolicy = $bindable(),
    acknowledgeBlocking = $bindable(),
    requestChangesPreviewOpen = $bindable(),
    reviewDetailsOpen = $bindable(),
    reviewAuditOpen = $bindable(),
    providerLink = $bindable(),
    commentDraft = $bindable(),
    commentCompose = $bindable(),
    reviewRationale = $bindable(),
    exportOpen = $bindable(),
    exportDestination = $bindable(),
    reviewHardBlockingMessages,
    reviewFooterIssues,
    issueLabel,
    showCommentRail,
    reviewDetailsLoading,
    providerLinkOpen,
    providerHandoff,
    commands,
    exportedDestination,
    reviewNoteOpen,
    reviewAllowed,
    remoteExport,
    onOpenFile,
    onRestore,
    onSelectCandidate,
    onComment,
    onToggleCommentRail,
    onExport,
    onAddProviderLink,
    onLoadMoreCommands,
    onSubmitComment,
    onCancelCompose,
    onResolveComment,
    onDeleteComment,
    onConfirmExport,
  }: Props = $props();

  let scrollEl = $state<HTMLDivElement | null>(null);
  let changesVisited = $state(false);
  const selectedRevision = $derived(review?.attempt_id);
  const candidate = $derived(review?.candidates.find(candidate => candidate.attempt_id === review.attempt_id));
  const areas = $derived(reviewAreas(review?.changed_files ?? []));
  let restoreGeneration = 0;
  let restoringScroll = false;
  async function restoreScroll() {
    const generation = restoreGeneration;
    await tick();
    if (generation !== restoreGeneration || !scrollEl) return;
    scrollEl.scrollTop = readingState.scroll[readingState.lens];
    restoringScroll = false;
  }
  $effect(() => {
    const lens = readingState.lens;
    const attempt = selectedRevision;
    void readingState.directory;
    void readingState.changeKind;
    const waitForDiff = untrack(() => lens === "changes" && !changesVisited && readingState.diff.expanded.length > 0);
    if (lens === "changes") changesVisited = true;
    restoreGeneration += 1;
    restoringScroll = true;
    if (!waitForDiff) untrack(() => { void restoreScroll(); });
    void attempt;
  });
  function setLens(lens: ReviewLens) {
    if (scrollEl) readingState.scroll[readingState.lens] = scrollEl.scrollTop;
    readingState.lens = lens;
  }
  function saveScroll() {
    if (scrollEl && !restoringScroll) readingState.scroll[readingState.lens] = scrollEl.scrollTop;
  }
  function showKind(kind: ReviewChangeKind | null) {
    showDirectory(null); readingState.changeKind = kind;
  }
  function showDirectory(directory: string | null) {
    readingState.changeKind = null;
    readingState.directory = directory;
    setLens("changes");
    readingState.scroll.changes = 0;
  }
  async function scrollToReviewFile(path: string) {
    showDirectory(null);
    await tick();
    const el = document.getElementById(`diff-file-${encodeURIComponent(path)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

        {#if review && (humanPhase === "review" || review.evidence_id)}
          <div class="flex min-h-0 flex-1 flex-col bg-surface-950/20">
            <nav class="review-page-lenses" aria-label="Review lenses">
              <button type="button" aria-pressed={readingState.lens === "overview"} onclick={() => setLens("overview")}>Summary</button>
              <button type="button" aria-pressed={readingState.lens === "changes"} onclick={() => setLens("changes")}>Changes <span>{review.changed_files.length}</span></button>
              <div class="review-lens-controls">
              {#if readingState.lens === "changes"}{#if readingState.changeKind}<button type="button" onclick={() => showKind(null)}>{changeKindLabels[readingState.changeKind!]} · Show all</button>{/if}<select aria-label="Filter changed files by directory" value={JSON.stringify(readingState.directory)} onchange={(event) => showDirectory(JSON.parse(event.currentTarget.value))}>
                <option value="null">All files · {review.changed_files.length}</option>
                {#each areas as area}<option value={JSON.stringify(area.directory)}>{area.label} · {area.count}</option>{/each}
              </select>{/if}
              {#if review.candidates.length > 1}<select aria-label="Review revision" value={review.attempt_id ?? ""} disabled={busy} onchange={(event) => onSelectCandidate(event.currentTarget.value)}>{#each review.candidates as candidate (candidate.attempt_id)}<option value={candidate.attempt_id}>Revision {candidate.attempt_seq} · {candidate.executor}</option>{/each}</select>{/if}
              </div>
            </nav>
            <div bind:this={scrollEl} onscroll={saveScroll} class="review-page-scroll min-h-0 flex-1 overflow-auto">
              <div
                class="review-canvas"
                class:review-canvas--with-rail={showCommentRail}
              >
                <div class="review-canvas-main">
                {#if readingState.lens !== "overview"}<ReviewPageHeading title={projectTitle} author={candidate?.executor ?? ""} revision={candidate ? `Revision ${candidate.attempt_seq}` : ""} target={sourceBranch} />{/if}
                {#if review.policy && (review.policy.violations.length || review.policy.capture_risks.length)}
                  <section class="review-policy" aria-label="Policy exceptions">
                    <CircleAlert size={15} strokeWidth={1.7} aria-hidden="true" />
                    <div class="min-w-0 flex-1">
                      <p>Review exceptions</p>
                      <ul>
                        {#each review.policy.violations as violation (violation.id)}
                          <li>
                            <button
                              type="button"
                              class="review-policy-path"
                              onclick={() => scrollToReviewFile(violation.path)}
                            >{violation.path}</button>
                            — {violation.detail}
                          </li>
                        {/each}
                        {#each review.policy.capture_risks as risk, riskIndex (`${risk.kind}:${"path" in risk ? risk.path : ""}:${riskIndex}`)}
                          <li>
                            {#if risk.kind === "secret_pattern"}
                              Possible secret in
                              <button
                                type="button"
                                class="review-policy-path"
                                onclick={() => scrollToReviewFile(risk.path)}
                              >{risk.path}</button>
                            {:else if risk.kind === "oversize_file"}
                              Large file
                              <button
                                type="button"
                                class="review-policy-path"
                                onclick={() => scrollToReviewFile(risk.path)}
                              >{risk.path}</button>
                            {:else}
                              These changes exceed the configured size limit
                            {/if}
                          </li>
                        {/each}
                      </ul>
                      {#if review.policy.violations.length}
                        <label>
                          <input type="checkbox" bind:checked={acknowledgePolicy} />
                          <span>I reviewed and accept these exceptions.</span>
                        </label>
                      {/if}
                    </div>
                  </section>
                {/if}

                {#if reviewHardBlockingMessages.length > 0 && !(review.policy?.violations.length)}
                  <section class="review-policy" aria-label="Blocking review issues">
                    <CircleAlert size={15} strokeWidth={1.7} aria-hidden="true" />
                    <div class="min-w-0 flex-1">
                      <p>Needs acknowledgment before approval</p>
                      <ul>
                        {#each reviewHardBlockingMessages as message (message)}
                          <li>{issueLabel(message)}</li>
                        {/each}
                      </ul>
                      <label>
                        <input type="checkbox" bind:checked={acknowledgeBlocking} />
                        <span>I reviewed these conditions and want to approve anyway.</span>
                      </label>
                    </div>
                  </section>
                {/if}

                {#if readingState.lens === "overview"}
                  <ReviewOverview title={projectTitle} {review} {originalRequest} {sourceBranch} onShowChanges={showDirectory} onShowKind={showKind} />
                {/if}
                {#if changesVisited}
                <div hidden={readingState.lens !== "changes"}>
                <ForgeReviewSurface
                  {review}
                  directory={readingState.directory}
                  changeKind={readingState.changeKind}
                  viewState={readingState.diff}
                  onReady={() => void restoreScroll()}
                  projectTitle={projectTitle}
                  {busy}
                  onOpenFile={onOpenFile}
                  onRestore={onRestore}
                  onSelectCandidate={onSelectCandidate}
                  onComment={onComment}
                  onToggleCommentRail={onToggleCommentRail}
                />
                </div>
                {/if}

                {#if readingState.lens === "changes" && review.decision?.rationale}
                  <p class="review-prior-note">
                    Prior decision note: <span>{review.decision.rationale}</span>
                  </p>
                {/if}

                {#if review.revision_brief || (review.unresolved_comment_count ?? 0) > 0 || (review.comments?.length ?? 0) > 0}
                  <section class="review-revision-brief" aria-label="Revision brief preview">
                    <button
                      type="button"
                      class="review-context-disclosure"
                      aria-expanded={requestChangesPreviewOpen}
                      onclick={() => (requestChangesPreviewOpen = !requestChangesPreviewOpen)}
                    >
                      <ChevronRight
                        size={13}
                        strokeWidth={2}
                        class="review-context-chevron {requestChangesPreviewOpen ? 'review-context-chevron--open' : ''}"
                      />
                      <span>Feedback for the next attempt</span>
                      <small>
                        {(review.unresolved_comment_count ?? 0) === 1
                          ? "1 open comment"
                          : `${review.unresolved_comment_count ?? 0} open comments`}
                      </small>
                    </button>
                    {#if requestChangesPreviewOpen}
                      <pre class="review-revision-brief-body">{review.revision_brief
                        || (review.comments ?? [])
                            .filter((comment) => !comment.resolved_at)
                            .map((comment) => `${comment.path}:${comment.start_line}\n${comment.body}`)
                            .join("\n\n")
                        || "No open comments yet."}</pre>
                    {/if}
                  </section>
                {/if}

                <section class="review-context" id="review-about">
                  <button
                    type="button"
                    class="review-context-disclosure"
                    aria-expanded={reviewDetailsOpen}
                    onclick={() => {
                      reviewDetailsOpen = !reviewDetailsOpen;
                    }}
                  >
                    <ChevronRight
                      size={13}
                      strokeWidth={2}
                      class="review-context-chevron {reviewDetailsOpen ? 'review-context-chevron--open' : ''}"
                    />
                    <span>About</span>
                    <small>Base, digest, and command log</small>
                  </button>

                  {#if reviewDetailsOpen}
                    <div class="review-context-body">
                      {#if reviewDetailsLoading}
                        <div class="review-context-loading">
                          <span class="review-context-loading-dot"></span>
                          Preparing context…
                        </div>
                      {/if}

                      <ReviewProvenanceStrip
                        {review}
                        baseRef={provenanceBaseRef}
                        onExport={onExport}
                      />

                      {#if providerLinkOpen && providerHandoff?.available}
                        <div class="review-link-compose">
                          <Link2 size={13} />
                          <input
                            type="url"
                            placeholder="Paste an issue, PR, or ticket URL"
                            bind:value={providerLink}
                            onkeydown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                onAddProviderLink();
                              }
                            }}
                          />
                          <button type="button" disabled={!providerLink.trim() || busy} onclick={() => onAddProviderLink()}>Add</button>
                        </div>
                      {/if}

                      {#if commands && commands.lines.length}
                        <div class="review-context-row review-context-row--stack">
                          <button
                            type="button"
                            class="review-context-row-button"
                            aria-expanded={reviewAuditOpen}
                            onclick={() => (reviewAuditOpen = !reviewAuditOpen)}
                          >
                            <span class="review-context-icon"><GitCommitHorizontal size={14} strokeWidth={1.7} /></span>
                            <span class="review-context-copy">
                              <span class="review-context-copy-title">Command log</span>
                              <span>{commands.lines.length} recorded {commands.lines.length === 1 ? "command" : "commands"}</span>
                            </span>
                            <ChevronRight
                              size={13}
                              class="review-context-row-chevron {reviewAuditOpen ? 'review-context-row-chevron--open' : ''}"
                            />
                          </button>
                          {#if reviewAuditOpen}
                            <div class="review-audit">
                              <p class="review-audit-revision">
                                {review.baseline_oid?.slice(0, 10)}… → {review.sealed_head_oid?.slice(0, 10)}…
                                {#if review.evidence_digest} · record {review.evidence_digest.slice(0, 16)}…{/if}
                              </p>
                              <pre>{commands.lines.join("\n")}</pre>
                              {#if commands.truncated}
                                <button type="button" disabled={busy} onclick={() => onLoadMoreCommands()}>Show more commands · {commands.lines.length} of {commands.total_lines}</button>
                              {/if}
                            </div>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/if}
                </section>

                {#if exportedDestination}
                  <p class="review-exported">
                    Project record saved at <span>{exportedDestination}</span>
                  </p>
                {/if}
                </div>

                {#if showCommentRail}
                  <ReviewCommentRail
                    comments={review.comments ?? []}
                    compose={commentCompose}
                    draft={commentDraft}
                    {busy}
                    onDraftChange={(value) => (commentDraft = value)}
                    onSubmit={onSubmitComment}
                    onCancelCompose={onCancelCompose}
                    onResolve={onResolveComment}
                    onDelete={onDeleteComment}
                    onJump={async (comment) => {
                      await scrollToReviewFile(comment.path);
                      const el = document.querySelector(
                        `[data-diff-line="${comment.start_line}"]`,
                      ) as HTMLElement | null;
                      el?.scrollIntoView({ behavior: "smooth", block: "center" });
                    }}
                  />
                {/if}
              </div>
            </div>

            {#if reviewAllowed && (reviewNoteOpen || reviewFooterIssues.length > 0)}
              <footer class="review-decision">
                {#if reviewNoteOpen}
                  <label class="sr-only" for="review-rationale">Review note</label>
                  <textarea
                    id="review-rationale"
                    rows="2"
                    class="review-note"
                    placeholder="Anything the next person should know?"
                    bind:value={reviewRationale}
                  ></textarea>
                {/if}
                {#if reviewFooterIssues.length}
                  <div class="review-decision-row">
                    <div class="review-decision-guidance">
                      <CircleAlert size={13} strokeWidth={1.7} />
                      <p title={reviewFooterIssues.join(" · ")}>
                        {issueLabel(reviewFooterIssues[0]!)}{reviewFooterIssues.length > 1 ? ` · ${reviewFooterIssues.length - 1} more` : ""}
                      </p>
                    </div>
                  </div>
                {/if}
              </footer>
            {/if}
          </div>
        {:else}
          <div class="flex min-h-0 flex-1 items-center justify-center p-8 text-center">
            <div class="max-w-sm">
              <p class="text-sm font-medium text-surface-200">Nothing to review yet</p>
              <p class="mt-1 text-xs leading-relaxed text-content-quiet">
                Keep working in a source tab. Review will become available when the project has a sealed change set.
              </p>
            </div>
          </div>
        {/if}

        {#if exportOpen}
          <div class="review-export-overlay">
            <button
              type="button"
              class="review-export-backdrop"
              aria-label="Close save project record"
              onclick={() => (exportOpen = false)}
            ></button>
            <div
              class="review-export-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="review-export-title"
              tabindex="-1"
            >
              <span class="review-export-icon"><Save size={17} strokeWidth={1.7} /></span>
              <div>
                <h4 id="review-export-title">Save a project record</h4>
                <p>
                  Keep a portable copy of the changes, activity, and decisions from this project.
                </p>
              </div>
              {#if remoteExport}
                <label for="export-destination">Folder on the connected computer</label>
                <input
                  id="export-destination"
                  placeholder="/path/to/project-record"
                  bind:value={exportDestination}
                />
                <small>Files stay on the connected computer.</small>
              {:else}
                <p class="review-export-path">{exportDestination}</p>
              {/if}
              <div class="review-export-actions">
                <button type="button" onclick={() => (exportOpen = false)}>Cancel</button>
                <button
                  type="button"
                  class="review-export-save"
                  disabled={busy || !exportDestination.trim()}
                  onclick={() => onConfirmExport()}
                >Save copy</button>
              </div>
            </div>
          </div>
        {/if}
