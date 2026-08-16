<script lang="ts">
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

  function scrollToReviewFile(path: string) {
    const el = document.getElementById(`diff-file-${encodeURIComponent(path)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

        {#if review && (humanPhase === "review" || review.evidence_id)}
          <div class="flex min-h-0 flex-1 flex-col bg-surface-950/20">
            <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
              <div
                class="review-canvas"
                class:review-canvas--with-rail={showCommentRail}
              >
                <div class="review-canvas-main">
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

                <ForgeReviewSurface
                  {review}
                  projectTitle={projectTitle}
                  {busy}
                  onOpenFile={onOpenFile}
                  onRestore={onRestore}
                  onSelectCandidate={onSelectCandidate}
                  onComment={onComment}
                  onToggleCommentRail={onToggleCommentRail}
                />

                {#if review.decision?.rationale}
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
                    onJump={(comment) => {
                      scrollToReviewFile(comment.path);
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

<style>
  .review-canvas {
    width: 100%;
    max-width: 88rem;
    margin: 0 auto;
  }

  .review-canvas--with-rail {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(14rem, 17rem);
    gap: 0.85rem;
    align-items: start;
  }

  .review-canvas-main {
    min-width: 0;
  }

  .review-revision-brief {
    margin-top: 0.85rem;
  }

  .review-revision-brief-body {
    margin-top: 0.45rem;
    max-height: 12rem;
    overflow: auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-950) / 0.35);
    padding: 0.65rem 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-family: var(--font-mono);
    font-size: 0.625rem;
    line-height: 1.45;
    white-space: pre-wrap;
  }

  @media (max-width: 960px) {
    .review-canvas--with-rail {
      grid-template-columns: 1fr;
    }
  }

  .review-policy {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    margin-top: 0.85rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.26);
    border-radius: 0.6rem;
    padding: 0.75rem;
    background: rgb(var(--color-warning-500) / 0.1);
    color: rgb(var(--theme-warning));
  }

  .review-policy p {
    font-size: 0.6875rem;
    font-weight: 600;
  }

  .review-policy ul {
    margin-top: 0.3rem;
    font-size: 0.625rem;
    line-height: 1.5;
    color: rgb(var(--theme-warning) / 0.72);
  }

  .review-policy-path {
    font-family: var(--font-mono);
  }

  .review-policy-path {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    text-decoration: underline;
    text-underline-offset: 0.12em;
    cursor: pointer;
  }

  .review-policy-path:hover {
    color: rgb(var(--color-surface-100));
  }

  .review-policy label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    margin-top: 0.55rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-warning));
  }

  .review-prior-note {
    margin-top: 0.75rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-prior-note span {
    color: rgb(var(--color-surface-200));
  }

  .review-context {
    margin-top: 0.85rem;
    padding: 0.2rem 0 0.8rem;
  }

  .review-context-disclosure {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.3rem 0.4rem 0.3rem 0.1rem;
    color: rgb(var(--theme-text-secondary));
    text-align: left;
    transition: color 140ms ease, background-color 140ms ease;
  }

  .review-context-disclosure:hover {
    background: rgb(var(--color-surface-800) / 0.3);
    color: rgb(var(--theme-text));
  }

  .review-context-disclosure > span {
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .review-context-disclosure small {
    margin-left: 0.25rem;
    font-size: 0.625rem;
    font-weight: 400;
    color: rgb(var(--theme-text-quiet));
  }

  :global(.review-context-chevron),
  :global(.review-context-row-chevron) {
    flex-shrink: 0;
    transition: transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  :global(.review-context-chevron--open),
  :global(.review-context-row-chevron--open) {
    transform: rotate(90deg);
  }

  .review-context-body {
    margin-top: 0.35rem;
    animation: review-context-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  @keyframes review-context-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .review-context-loading {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.55rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-context-loading-dot {
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 50%;
    background: rgb(var(--color-primary-400));
    animation: review-context-pulse 1.2s ease-in-out infinite;
  }

  @keyframes review-context-pulse {
    50% {
      opacity: 0.35;
      transform: scale(0.75);
    }
  }

  .review-context-row,
  .review-context-row-button {
    display: grid;
    grid-template-columns: 1.75rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
  }

  .review-context-row {
    min-height: 2.8rem;
    border-radius: 0.5rem;
    padding: 0.45rem 0.55rem;
    transition: background-color 140ms ease;
  }

  .review-context-row:hover {
    background: rgb(var(--color-surface-800) / 0.2);
  }

  .review-context-row--stack {
    display: block;
  }

  .review-context-row-button {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    text-align: left;
  }

  .review-context-icon {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--theme-text-quiet));
  }

  .review-context-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

  .review-context-copy-title {
    overflow: hidden;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-context-copy > span:not(.review-context-copy-title) {
    overflow: hidden;
    margin-top: 0.1rem;
    font-size: 0.6875rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .review-audit button {
    border: 0;
    background: transparent;
    padding: 0.15rem 0.3rem;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-audit button:hover {
    color: rgb(var(--theme-link));
  }

  .review-link-compose {
    display: flex;
    max-width: 34rem;
    align-items: center;
    gap: 0.4rem;
    margin: 0.45rem 0 0;
    border-radius: 0.45rem;
    padding: 0.3rem 0.4rem;
    background: rgb(var(--color-surface-800) / 0.28);
    color: rgb(var(--theme-text-quiet));
  }

  .review-link-compose input {
    appearance: none;
    min-width: 0;
    flex: 1;
    border: 0;
    background: transparent;
    padding: 0.15rem;
    box-shadow: none;
    outline: none;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
  }

  .review-link-compose input::placeholder {
    color: rgb(var(--theme-text-faint));
  }

  .review-link-compose button {
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.25rem 0.4rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-link));
  }

  .review-link-compose button:disabled {
    color: rgb(var(--theme-text-faint));
  }

  .review-audit {
    margin: 0.55rem 0 0 2.3rem;
  }

  .review-audit-revision {
    margin-bottom: 0.35rem;
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .review-audit pre {
    max-height: 12rem;
    overflow: auto;
    border-radius: 0.45rem;
    padding: 0.6rem;
    background: rgb(0 0 0 / 0.24);
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-tertiary));
  }

  .review-exported {
    margin: 0.4rem 0;
    font-size: 0.625rem;
    color: rgb(var(--theme-success));
  }

  .review-exported span {
    font-family: var(--font-mono);
  }

  .review-decision {
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
    padding: 0.6rem 1rem;
    background: rgb(var(--color-surface-900) / 0.94);
    box-shadow: 0 -0.5rem 1.5rem rgb(0 0 0 / 0.14);
    backdrop-filter: blur(12px);
  }

  .review-note {
    appearance: none;
    width: 100%;
    margin-bottom: 0.55rem;
    resize: none;
    border: 0;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-800) / 0.4);
    padding: 0.55rem 0.65rem;
    box-shadow: none;
    outline: none;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .review-note::placeholder {
    color: rgb(var(--theme-text-faint));
  }

  .review-decision-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .review-decision-guidance {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.45rem;
    color: rgb(var(--theme-warning));
  }

  .review-decision-guidance p {
    overflow: hidden;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-export-overlay {
    position: absolute;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }

  .review-export-backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    background: rgb(0 0 0 / 0.5);
    backdrop-filter: blur(3px);
  }

  .review-export-dialog {
    position: relative;
    display: grid;
    width: min(26rem, 100%);
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.7rem 0.8rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.8rem;
    padding: 1rem;
    background: rgb(var(--color-surface-900) / 0.98);
    box-shadow: 0 1.5rem 4rem rgb(0 0 0 / 0.4);
  }

  .review-export-icon {
    display: inline-flex;
    width: 2rem;
    height: 2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--theme-link));
  }

  .review-export-dialog h4 {
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .review-export-dialog p {
    margin-top: 0.2rem;
    font-size: 0.6875rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .review-export-dialog label,
  .review-export-dialog input,
  .review-export-dialog small,
  .review-export-path,
  .review-export-actions {
    grid-column: 1 / -1;
  }

  .review-export-dialog label {
    margin-top: 0.35rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .review-export-dialog input {
    appearance: none;
    border: 0;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.5);
    padding: 0.5rem 0.6rem;
    box-shadow: none;
    outline: none;
    font-family: var(--font-mono);
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
  }

  .review-export-dialog input::placeholder {
    color: rgb(var(--theme-text-faint));
  }

  .review-export-dialog small {
    margin-top: -0.45rem;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .review-export-dialog .review-export-path {
    overflow: hidden;
    margin-top: 0.35rem;
    border-radius: 0.45rem;
    padding: 0.5rem 0.6rem;
    background: rgb(var(--color-surface-800) / 0.35);
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: rgb(var(--theme-text-tertiary));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-export-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
    margin-top: 0.35rem;
  }

  .review-export-actions button {
    border: 1px solid transparent;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.4rem 0.6rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .review-export-actions button:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .review-export-actions .review-export-save {
    border-color: rgb(var(--color-primary-500) / 0.3);
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--color-primary-200));
  }

  .review-export-actions button:disabled {
    opacity: 0.35;
  }

  @media (max-width: 640px) {
    .review-context-disclosure small {
      display: none;
    }

    .review-context-row,
    .review-context-row-button {
      grid-template-columns: 1.75rem minmax(0, 1fr);
    }

    :global(.review-context-row-chevron) {
      grid-column: 2;
      justify-self: flex-start;
    }

    .review-decision-row {
      align-items: flex-start;
      flex-direction: column;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .review-context-body,
    :global(.review-context-chevron),
    :global(.review-context-row-chevron),
    .review-context-loading-dot {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
