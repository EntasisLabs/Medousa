<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { hostedReviews } from "$lib/code/hostedReviews.svelte";
  import { hostedReviewCache, type ReviewCacheOptions } from "$lib/code/hostedReviewCache";
  import ReviewPageHeading from "./ReviewPageHeading.svelte";
  import { CircleCheck, CircleAlert, CircleDashed, Files, ArrowUpRight, RefreshCw, ChevronRight, FileCode2 } from "@lucide/svelte";
  import HostedReviewActions from "./HostedReviewActions.svelte";
  import { mutateHostedReview } from "$lib/code/hostedReviews";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import DiffViewOptions from "$lib/components/diff/DiffViewOptions.svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { queryHostedReviews, type HostedReviewRef, type HostedReview, type HostedReviewFile, type HostedReviewDiscussionEntry, type HostedReviewCheck } from "$lib/code/hostedReviews";
  import { openInBrowser } from "$lib/utils/openInBrowser";
  import { reviewDraftKey, readReviewDraft, saveReviewDraft } from "$lib/code/hostedReviewDraft";
  import { hostedReviewHunks } from "$lib/code/hostedReviewPatch";
  import type { ReviewInlineComment, ReviewProviderResult, ReviewProviderQuery } from "$lib/types/generated/daemon_api";

  let { reference, connectionId = null, scope = "" }: { reference: HostedReviewRef; connectionId?: string | null; scope?: string } = $props();
  let actionControls = $state<{ openAction: (mode: "comment" | "merge") => void } | null>(null);
  const providerLabel = $derived(hostedReviews.providers.find(provider => provider.id === reference.provider_id)?.label ?? reference.host);
  let review = $state<HostedReview | null>(null);
  let lens = $state<"summary" | "changes" | "discussion" | "checks">("summary");
  let files = $state<HostedReviewFile[]>([]), entries = $state<HostedReviewDiscussionEntry[]>([]), checks = $state<HostedReviewCheck[]>([]);
  let fileCursor = $state<string | null>(null), discussionCursor = $state<string | null>(null), checksCursor = $state<string | null>(null);
  let loaded = $state({ changes: false, discussion: false, checks: false });
  let loading = $state(false), loadingContent = $state(false);
  let showContentLoading = $state(false);
  let diffMode = $state<"inline" | "side">("inline");
  let diffDensity = $state<"comfortable" | "compact">("compact");
  let diffWrap = $state(true);
  let error = $state<string | null>(null), activePath = $state<string | null>(null), patch = $state<string | null>(null), patchMessage = $state<string | null>(null);
  let draftRevision = $state<string | null>(null);
  let drafts = $state<ReviewInlineComment[]>([]);
  let replyTarget = $state<string | null>(null);
  let replyId = $state<string | null>(null), replyBody = $state("");
  let writing = $state(false), notice = $state<string | null>(null);
  let checkId = $state<string | null>(null), checkDetails = $state<Extract<ReviewProviderResult, { kind: "check_details" }> | null>(null);
  let checkLoading = $state(false);
  let detailGeneration = 0;
  let generation = 0, selection = 0;
  const tabs = ["summary", "changes", "discussion", "checks"] as const;
  const cacheContext = () => ({ scope, provider: reference.provider_id, host: reference.host, connectionId });
  const query = (request: ReviewProviderQuery["request"], options?: ReviewCacheOptions) => hostedReviewCache.query(cacheContext(), request, () => queryHostedReviews(reference.provider_id, reference.host, request, connectionId), options);
  const displayFiles = $derived(activePath && patch !== null ? [{ path: activePath, hunks: hostedReviewHunks(patch) }] : []);

  async function openProviderLink(event: MouseEvent, url: string) {
    event.preventDefault();
    const current = generation;
    try { await openInBrowser(url, { openedBy: "user", newTab: true }); }
    catch { if (current === generation) error = "Could not open the provider page."; }
  }

  function collectionRequest(target: Exclude<typeof lens, "summary">, more = false): ReviewProviderQuery["request"] {
    return target === "changes" ? { operation: "files", review: reference, revision: review!.head_revision, cursor: more ? fileCursor : null }
      : target === "checks" ? { operation: "checks", review: reference, revision: review!.head_revision, cursor: more ? checksCursor : null }
      : { operation: "discussion", review: reference, cursor: more ? discussionCursor : null };
  }
  function nextCollectionPage(result: ReviewProviderResult): ReviewProviderQuery["request"] | null {
    if (result.kind === "discussion" && result.entries.length < 100 && result.next_cursor) return { operation: "discussion", review: reference, cursor: result.next_cursor };
    if (result.kind === "checks" && result.checks.length < 100 && result.next_cursor) return { operation: "checks", review: reference, revision: review!.head_revision, cursor: result.next_cursor };
    return null;
  }
  function combinePages(result: ReviewProviderResult, next: ReviewProviderResult): ReviewProviderResult {
    if (result.kind === "discussion" && next.kind === "discussion") return { ...next, entries: [...result.entries, ...next.entries] };
    if (result.kind === "checks" && next.kind === "checks") return { ...next, checks: [...result.checks, ...next.checks] };
    throw new Error("Unexpected review collection response.");
  }
  function cachedCollection(target: Exclude<typeof lens, "summary">) {
    const first = hostedReviewCache.peek(cacheContext(), collectionRequest(target));
    if (!first) return null;
    let { result, fresh } = first;
    for (let page = 0; page < 2; page++) {
      const request = nextCollectionPage(result);
      if (!request) break;
      const next = hostedReviewCache.peek(cacheContext(), request);
      if (!next) { fresh = false; break; }
      result = combinePages(result, next.result); fresh &&= next.fresh;
    }
    return { result, fresh };
  }
  function applyCollection(result: ReviewProviderResult, target: Exclude<typeof lens, "summary">, more = false) {
    if (target === "changes" && result.kind === "files") { files = more ? [...files, ...result.files] : result.files; fileCursor = result.next_cursor ?? null; }
    else if (target === "checks" && result.kind === "checks") { checks = [...new Map((more ? [...checks, ...result.checks] : result.checks).map(check => [check.id, check])).values()]; checksCursor = result.next_cursor ?? null; }
    else if (target === "discussion" && result.kind === "discussion") { entries = [...new Map((more ? [...entries, ...result.entries] : result.entries).map(entry => [entry.id, entry])).values()].sort((a, b) => Date.parse(a.created_at) - Date.parse(b.created_at)); discussionCursor = result.next_cursor ?? null; }
    else throw new Error("The provider returned an unexpected review response.");
    loaded = { ...loaded, [target]: true };
    showContentLoading = false;
  }
  async function refresh(force = true) {
    if (force) hostedReviewCache.invalidateReview(cacheContext(), reference);
    const current = ++generation;
    loading = true; error = null; loadingContent = false; showContentLoading = false;
    if (activePath && patch === null) { selection++; activePath = null; patchMessage = null; }
    if (checkId && !checkDetails) { detailGeneration++; checkId = null; checkLoading = false; }
    function apply(result: ReviewProviderResult) {
      if (current !== generation || result.kind !== "review") return;
      const changed = review?.head_revision !== result.review.head_revision || review?.base_revision !== result.review.base_revision;
      review = result.review;
      if (changed) {
        selection++; detailGeneration++; activePath = null; patch = null; checkId = null; checkDetails = null; checkLoading = false;
        files = []; entries = []; checks = []; fileCursor = discussionCursor = checksCursor = null;
      }
      loaded = { changes: false, discussion: false, checks: false };
      if (!force) {
        const target = lens === "summary" ? "checks" : lens;
        const cached = cachedCollection(target);
        if (cached) applyCollection(cached.result, target);
      }
    }
    try {
      const result = await query({ operation: "get", review: reference }, { force, onCached: apply });
      if (current !== generation) return;
      if (result.kind !== "review") throw new Error("The provider returned an unexpected review response.");
      apply(result);
    } catch (cause) { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); return; }
    finally { if (current === generation) loading = false; }
    if (current === generation && review) await loadLens(false, lens === "summary" ? "checks" : lens, force);
  }
  async function loadLens(more = false, target = lens, force = false) {
    if (!review || target === "summary" || loadingContent) return;
    const current = generation;
    const request = collectionRequest(target, more);
    const cached = force ? null : more ? hostedReviewCache.peek(cacheContext(), request) : cachedCollection(target);
    if (!more && loaded[target] && cached?.fresh) return;
    const hadContent = loaded[target] || (target === "changes" ? files.length > 0 : target === "checks" ? checks.length > 0 : entries.length > 0);
    loadingContent = true; showContentLoading = !hadContent && !cached; error = null;

    if (cached && !more && !hadContent) applyCollection(cached.result, target);
    try {
      let result = await query(request, { force }); if (current !== generation) return;
      // Follow a few small provider pages; leave large collections lazy.
      for (let page = 0; page < 2; page++) {
        const request = nextCollectionPage(result);
        if (!request) break;
        const next = await query(request, { force }).catch(cause => { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); return null; });
        if (current !== generation) return;
        if (!next) break;
        result = combinePages(result, next);
      }
      applyCollection(result, target, more);
    } catch (cause) { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (current === generation) { loadingContent = false; if (lens !== target) void loadLens(); } }
  }
  async function selectLens(next: typeof lens) { lens = next; await loadLens(); }
  async function openFile(path: string) {
    if (!review) return;
    if (activePath === path) { selection++; activePath = null; patch = null; patchMessage = null; return; }
    const current = generation, selected = ++selection;
    activePath = path; patch = null; patchMessage = "Loading patch…"; error = null;
    try {
      const result = await query({ operation: "diff", review: reference, revision: review.head_revision, path }, { onCached: result => {
        if (current === generation && selected === selection && result.kind === "diff") { patch = result.patch ?? null; patchMessage = result.message ?? null; }
      } });
      if (current !== generation || selected !== selection) return;
      if (result.kind !== "diff") throw new Error("The provider returned an unexpected patch response.");
      patch = result.patch ?? null; patchMessage = result.message ?? null;
    } catch (cause) { if (current === generation && selected === selection) patchMessage = cause instanceof Error ? cause.message : String(cause); }
  }
  async function postReply(event: SubmitEvent) {
    event.preventDefault(); if (!replyId || !replyBody.trim() || writing) return;
    const current = generation; writing = true; error = null;
    try {
      const result = await mutateHostedReview(reference.provider_id, reference.host, { operation: "comment", review: reference, body: replyBody, reply_to: replyTarget ?? replyId }, connectionId);
      if (current !== generation) return;
      if (result.kind !== "updated") throw new Error("The provider did not confirm the reply.");
      replyId = null; replyBody = ""; notice = result.message;
      hostedReviewCache.invalidateReview(cacheContext(), reference);
      loaded = { ...loaded, discussion: false }; await loadLens(false, "discussion", true);
    } catch (cause) { if (current === generation) error = `${cause instanceof Error ? cause.message : String(cause)} Refresh before submitting again.`; }
    finally { writing = false; }
  }
  function stageComment(input: { path: string; side: "old" | "new"; line: number; content: string }) {
    if (drafts.reduce((total, c) => total + new TextEncoder().encode(c.body).length, 0) + new TextEncoder().encode(input.content).length > 96000) { error = "Submit these pending comments before adding more text."; return; }
    if (drafts.length >= 100) { error = "Submit this review before adding more than 100 comments."; return; }
    if (!drafts.length) draftRevision = review?.head_revision ?? null;
    if (draftRevision !== review?.head_revision) { error = "Remove pending comments from the previous revision before adding new ones."; return; }
    drafts = [...drafts, { path: input.path, side: input.side, line: input.line, body: input.content }];
    notice = "Inline comment added to your review. Choose Review to submit it.";
  }
  async function inspectCheck(id: string) {
    if (!review) return;
    if (checkId === id) { detailGeneration++; checkId = null; checkDetails = null; checkLoading = false; return; }
    const current = generation, detail = ++detailGeneration; checkId = id; checkDetails = null; checkLoading = true; error = null;
    try {
      const result = await query({ operation: "check_details", review: reference, revision: review.head_revision, check_id: id }, { onCached: result => {
        if (current === generation && detail === detailGeneration && result.kind === "check_details") { checkDetails = result; checkLoading = false; }
      } });
      if (current !== generation || detail !== detailGeneration) return;
      if (result.kind !== "check_details") throw new Error("The provider returned unexpected check details.");
      checkDetails = result;
    } catch (cause) { if (current === generation && detail === detailGeneration) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (current === generation && detail === detailGeneration) checkLoading = false; }
  }
  async function runCheck(check: HostedReviewCheck, cancel = false, failedOnly = false) {
    if (!review || writing) return;
    const current = generation;
    writing = true; error = null;
    try {
      const request = cancel ? { operation: "cancel_check" as const, review: reference, revision: review.head_revision, check_id: check.id } : { operation: "rerun_check" as const, review: reference, revision: review.head_revision, check_id: check.id, failed_only: failedOnly };
      const result = await mutateHostedReview(reference.provider_id, reference.host, request, connectionId);
      if (current !== generation) return;
      if (result.kind !== "updated") throw new Error("The provider did not confirm this workflow action.");
      notice = result.message; hostedReviewCache.invalidateReview(cacheContext(), reference);
      loaded = { ...loaded, checks: false }; await loadLens(false, "checks", true);
    } catch (cause) { if (current === generation) error = `${cause instanceof Error ? cause.message : String(cause)} Refresh before submitting again.`; }
    finally { writing = false; }
  }
  onMount(() => {
    const saved = readReviewDraft(reviewDraftKey(scope, reference, connectionId, "inline"));
    if (saved) {
      if (Array.isArray(saved.comments)) drafts = saved.comments.filter((c): c is ReviewInlineComment => c && typeof c.path === "string" && typeof c.body === "string" && typeof c.line === "number" && (c.side === "old" || c.side === "new")).slice(0, 100);
      if (typeof saved.revision === "string") draftRevision = saved.revision;
      if (tabs.includes(saved.lens as typeof lens)) lens = saved.lens as typeof lens;
      if (typeof saved.replyId === "string") replyId = saved.replyId;
      if (typeof saved.replyTarget === "string") replyTarget = saved.replyTarget;
      if (typeof saved.replyBody === "string") replyBody = saved.replyBody;
    }
    void refresh(false);
  });
  onDestroy(() => {
    saveReviewDraft(reviewDraftKey(scope, reference, connectionId, "inline"), { comments: drafts, revision: draftRevision, lens, replyId, replyTarget, replyBody });
    generation++; selection++; detailGeneration++;
  });
</script>

<section class="hosted-review" aria-label="Hosted review">
  <header class="review-toolbar">
    <div class="source"><span>{providerLabel}</span><span>{reference.repository.split("/").at(-1)} #{reference.native_id}</span>{#if review}<span class="review-state" class:merged={review.state === "merged"}>{review.state === "draft" ? "Draft" : review.state === "merged" ? "Merged" : review.state === "closed" ? "Closed" : "Open"}</span>{/if}</div>
    <div class="actions"><button type="button" aria-label="Refresh review" disabled={loading} onclick={() => void refresh()}><RefreshCw size={14} /></button>{#if review}<HostedReviewActions bind:this={actionControls} {review} {connectionId} {scope} {drafts} {draftRevision} onupdated={refresh} onclear={() => { drafts = []; draftRevision = null; }} />{/if}</div>
  </header>
  <nav aria-label="Review lenses">{#each tabs as tab}<button type="button" class:active={lens === tab} aria-pressed={lens === tab} onclick={() => void selectLens(tab)}>{tab[0].toUpperCase() + tab.slice(1)}{tab === "changes" && review ? ` ${review.changed_files}` : ""}</button>{/each}{#if lens === "changes"}<div class="diff-options"><DiffViewOptions bind:mode={diffMode} bind:density={diffDensity} bind:wrap={diffWrap} /></div>{/if}{#if !review?.actions?.length}<span>Read only</span>{/if}</nav>
  <div class="content">
    {#if notice}<p class="message" role="status">{notice}</p>{/if}
    {#if error}<p class="message" role="alert">{error}</p>{/if}
    {#if loading && !review}<p class="message" role="status">Loading review…</p>
    {:else if review}
      {#if lens !== "summary"}<div class="lens-heading" class:lens-heading--changes={lens === "changes"}><ReviewPageHeading title={review.title} author={review.author} branch={review.head_branch} target={review.base_branch} /></div>{/if}
      {#if lens === "summary"}
        <article class="summary"><div class="body">
          <ReviewPageHeading title={review.title} author={review.author} branch={review.head_branch} target={review.base_branch} updatedAt={review.updated_at} />
          <div class="description">{#if review.body.trim()}<MarkdownContent presentation="document" content={review.body} openLinksInWeb linkBaseUrl={review.url} />{:else}<p class="quiet">No description provided.</p>{/if}</div>
          {#if review.actions?.includes("comment")}<button type="button" class="comment-entry" onclick={() => actionControls?.openAction("comment")}>Leave a comment…</button>{/if}
        </div><aside aria-label="Review status">
          <section><h2>Changes</h2><p class="change-count"><span class="change-total"><span class="review-symbol" aria-hidden="true"><Files size={14}/></span><span>{review.changed_files} {review.changed_files === 1 ? "file" : "files"}</span></span><span class="change-stats"><span class="added">+{review.additions}</span><span class="removed">−{review.deletions}</span></span></p><button class="text-action" type="button" onclick={() => void selectLens("changes")}>Browse changes</button></section>
          <section><h2>Reviewers</h2>{#each review.requested_reviewers ?? [] as reviewer}<p>{reviewer}</p>{:else}<p class="quiet">No reviewers requested</p>{/each}</section>
          <section><h2>Checks</h2>{#each checks.slice(0, 4) as check (check.id)}<p class="check-preview" class:passed={check.conclusion === "success" || check.status === "success"} class:failed={["failure", "timed_out", "error", "action_required"].includes(check.conclusion ?? check.status)}><span class="review-symbol" aria-hidden="true">{#if check.conclusion === "success" || check.status === "success"}<CircleCheck size={14}/>{:else if ["failure", "timed_out", "error", "action_required"].includes(check.conclusion ?? check.status)}<CircleAlert size={14}/>{:else}<CircleDashed size={14}/>{/if}</span><span class="check-copy">{check.name}<small>{(check.conclusion ?? check.status).replaceAll("_", " ")}</small></span></p>{:else}<p class="quiet">{!loaded.checks && loadingContent ? "Loading checks…" : loaded.checks ? "No checks recorded" : "Checks unavailable"}</p>{/each}<button class="text-action" type="button" onclick={() => void selectLens("checks")}>View all checks</button></section>
          <section><h2>Merge status</h2><p class="merge-status" class:passed={review.merge_state === "clean"}>{#if review.merge_state === "clean"}<span class="review-symbol" aria-hidden="true"><CircleCheck size={14}/></span>{/if}<span>{review.state === "merged" ? "Merged" : review.merge_state === "clean" ? "Can merge without conflicts" : review.merge_state?.replaceAll("_", " ") || "Not yet determined"}</span></p>{#if review.actions?.includes("merge")}<button type="button" class="merge-button" onclick={() => actionControls?.openAction("merge")}>Merge…</button>{/if}<details><summary>Branch details</summary><p>{review.head_branch} → {review.base_branch}</p><code>{review.head_revision.slice(0, 12)}</code></details></section>
          <section><h2>Repository</h2><p>{reference.repository}</p><a class="text-action" href={review.url} onclick={(event) => void openProviderLink(event, review!.url)} onauxclick={(event) => { if (event.button === 1) void openProviderLink(event, review!.url); }} target="_blank" rel="noopener noreferrer">Open on {providerLabel}<ArrowUpRight size={12}/></a></section>
        </aside></article>
      {:else if lens === "changes"}
        {#if drafts.length}<div class="draft-comments"><h2>Pending review · {drafts.length} comments</h2>{#each drafts as draft, index}<div><span>{draft.path}:{draft.line} ({draft.side}) — {draft.body}</span><button type="button" aria-label={`Remove pending comment ${index + 1}`} onclick={() => drafts = drafts.filter((_, i) => i !== index)}>Remove</button></div>{/each}<p>These comments have not been posted. Choose Review to submit them.</p></div>{/if}
        <div class="files">{#each files as file (file.path)}<section class="file"><button type="button" class="file-row" aria-expanded={activePath === file.path} onclick={() => void openFile(file.path)}><ChevronRight size={13} class={activePath === file.path ? "file-chevron-open" : ""} /><FileCode2 size={13}/><span>{file.path}</span><small>{file.status}</small><span class="added">+{file.additions}</span><span class="removed">−{file.deletions}</span></button>{#if activePath === file.path}{#if displayFiles.length}<DiffStack files={displayFiles} embedded mode={diffMode} density={diffDensity} wrap={diffWrap} chrome="none" onComment={review.actions?.includes("submit_review") ? stageComment : undefined} />{:else}<p class="message" role="status">{patchMessage ?? "No text patch available."}</p>{/if}{/if}</section>{/each}
          {#if loaded.changes && !fileCursor && files.length < review.changed_files}<p class="message">The provider returned a limited file list. Open the review on {reference.host} to see the remaining files.</p>{/if}
          {#if fileCursor}<button class="more" type="button" disabled={loadingContent} onclick={() => void loadLens(true)}>Load more files</button>{/if}
        </div>
      {:else if lens === "discussion"}
        <div class="discussion">{#each entries as entry (entry.id)}<article><header><strong>{entry.author}</strong><small>{entry.kind.replaceAll("_", " ")}{entry.reply_to ? " · reply" : ""} · {new Date(entry.created_at).toLocaleString()}</small><a href={entry.url} onclick={(event) => void openProviderLink(event, entry.url)} onauxclick={(event) => { if (event.button === 1) void openProviderLink(event, entry.url); }} target="_blank" rel="noopener noreferrer"><ArrowUpRight size={14} /><span class="sr-only">Open comment</span></a></header>{#if entry.path}<p class="quiet">{entry.path}{entry.line ? `:${entry.line}` : ""}</p>{/if}{#if entry.body.trim()}<MarkdownContent presentation="document" content={entry.body} openLinksInWeb linkBaseUrl={entry.url} />{/if}
          {#if entry.kind === "inline_comment" && review.actions?.includes("comment")}
            <button type="button" class="reply" disabled={writing} onclick={() => { replyId = entry.id; replyTarget = entry.reply_to ?? entry.id; replyBody = ""; }}>Reply to thread</button>
            {#if replyId === entry.id}<form class="reply-form" onsubmit={postReply}><label>Reply<textarea aria-label="Thread reply" bind:value={replyBody} required maxlength={60000} rows={3}></textarea></label><button type="submit" disabled={writing}>{writing ? "Posting…" : "Post reply"}</button><button type="button" disabled={writing} onclick={() => replyId = null}>Cancel</button></form>{/if}
          {/if}</article>{/each}
          {#if discussionCursor && entries.length < 1000}<button class="more" type="button" disabled={loadingContent} onclick={() => void loadLens(true)}>Load more discussion</button>{:else if loaded.discussion && !entries.length}<p class="message">No discussion recorded.</p>{/if}
        </div>
      {:else}
        <div class="checks">{#each checks as check (check.id)}<section class="check"><div class="check-row"><strong>{check.name}<small>{check.id.startsWith("workflows:") ? "Workflow" : check.id.startsWith("statuses:") ? "Commit status" : "Check"}</small></strong><span class:success={check.conclusion === "success" || check.status === "success"} class:failure={["failure", "timed_out", "error", "action_required"].includes(check.conclusion ?? check.status)}>{(check.conclusion ?? check.status).replaceAll("_", " ")}</span>{#if !check.id.startsWith("statuses:")}<button type="button" aria-expanded={checkId === check.id} onclick={() => void inspectCheck(check.id)}>Details</button>{/if}{#if check.url}<a href={check.url} onclick={(event) => void openProviderLink(event, check.url!)} onauxclick={(event) => { if (event.button === 1) void openProviderLink(event, check.url!); }} target="_blank" rel="noopener noreferrer"><ArrowUpRight size={14}/><span class="sr-only">Open check</span></a>{/if}</div>
            {#if check.id.startsWith("workflows:")}<div class="workflow-actions">{#if check.status === "completed" && review.actions?.includes("rerun_check")}<button type="button" disabled={writing} onclick={() => void runCheck(check)}>Rerun all jobs</button>{#if check.conclusion === "failure"}<button type="button" disabled={writing} onclick={() => void runCheck(check, false, true)}>Rerun failed jobs</button>{/if}{:else if review.actions?.includes("cancel_check")}<button type="button" disabled={writing} onclick={() => void runCheck(check, true)}>Cancel run</button>{/if}</div>{/if}
            {#if checkId === check.id}<article class="check-details">{#if checkLoading}<p>Loading check details…</p>{:else if checkDetails}<h2>{checkDetails.title}</h2><MarkdownContent presentation="document" content={checkDetails.summary} openLinksInWeb linkBaseUrl={check.url ?? review.url} />{#if checkDetails.text}<MarkdownContent presentation="document" content={checkDetails.text} openLinksInWeb linkBaseUrl={check.url ?? review.url} />{/if}{#each checkDetails.annotations as annotation}<pre>{annotation}</pre>{/each}{/if}</article>{/if}
          </section>{/each}{#if checksCursor}<button class="more" type="button" disabled={loadingContent} onclick={() => void loadLens(true)}>Load more checks</button>{:else if loaded.checks && !checks.length}<p class="message">No checks recorded.</p>{/if}</div>
      {/if}
      {#if loadingContent && showContentLoading}<p class="message" role="status">Loading…</p>{/if}
    {/if}
  </div>
</section>

<style>
  .hosted-review { display:flex; flex-direction:column; height:100%; min-height:0; color:rgb(var(--theme-text)); }
  header { display:flex; align-items:center; gap:1rem; }
  .review-toolbar { justify-content:space-between; padding:.6rem 1.25rem; flex-shrink:0; gap:.6rem; }
  .source { display:flex; flex-wrap:wrap; gap:.5rem; color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; }
  .source > span:first-child { color:rgb(var(--theme-text-secondary)); }
  .review-state { color:rgb(var(--theme-success)); }
  .review-state.merged { color:rgb(var(--color-primary-300)); }

  small,.quiet { font-size:.75rem; color:rgb(var(--theme-text-tertiary)); }
  button,a { color:inherit; } button { cursor:pointer; border:0; background:transparent; } button:disabled { opacity:.5; cursor:wait; }
  .actions { display:flex; align-items:center; gap:.5rem; }
  nav { display:flex; gap:1rem; align-items:center; padding:0 1.25rem; border-bottom:1px solid rgb(var(--theme-border)/.5); } nav button { padding:.65rem 0; font-size:.8rem; border-bottom:2px solid transparent; color:rgb(var(--theme-text-tertiary)); } nav button.active { border-color:rgb(var(--theme-text-secondary)); color:rgb(var(--theme-text)); } nav > span { margin-left:auto; font-size:.6875rem; color:rgb(var(--theme-text-tertiary)); }
  .content { flex:1; min-height:0; overflow:auto; padding:1.5rem 1.25rem 2rem; container-type:inline-size; } .summary { display:grid; grid-template-columns:minmax(0,1fr) 11rem; gap:2rem; max-width:64rem; margin:0 auto; } .body { min-width:0; } aside h2 { margin:0 0 .5rem; font-size:.75rem; font-weight:500; color:rgb(var(--theme-text-tertiary)); } aside p { font-size:.75rem; margin:0 0 .5rem; overflow-wrap:anywhere; }
  .added { color:rgb(var(--color-green-400)); } .removed { color:rgb(var(--color-red-400)); } .file { border-bottom:1px solid rgb(var(--theme-border)/.4); } .file-row { display:flex; align-items:center; gap:.6rem; width:100%; text-align:left; padding:.55rem .4rem; font-size:.75rem; } .file-row > span:first-of-type { flex:1; overflow-wrap:anywhere; } .file-row small { font-size:.625rem; }
  .draft-comments { padding:.8rem; margin-bottom:1rem; border:1px solid rgb(var(--theme-border)/.5); border-radius:.5rem; font-size:.75rem; } .draft-comments h2,.check-details h2 { font-size:.85rem; } .draft-comments > div { display:flex; gap:1rem; margin:.5rem 0; } .draft-comments span { flex:1; overflow-wrap:anywhere; }
  .reply,.workflow-actions button,.check-row button,.draft-comments button { font-size:.75rem; padding:.3rem .5rem; color:rgb(var(--color-primary-300)); }
  .reply-form { padding:.75rem 0; } .reply-form textarea { display:block; width:100%; padding:.5rem; margin:.5rem 0; background:rgb(var(--theme-text)/.035); color:inherit; border:1px solid rgb(var(--theme-border)/.6); border-radius:.4rem; } .reply-form button { padding:.5rem; }
  .check { border-bottom:1px solid rgb(var(--theme-border)/.5); } .check-row { display:flex; align-items:center; gap:.75rem; padding:.75rem 0; font-size:.75rem; } .check-row strong { flex:1; font-weight:500; } .check-row small { display:block; margin-top:.25rem; } .check-row .success { color:rgb(var(--color-green-400)); } .check-row .failure { color:rgb(var(--color-red-400)); } .check-details { padding:.5rem 0 1rem; } .check-details pre { white-space:pre-wrap; overflow-wrap:anywhere; font-size:.75rem; padding:.6rem; background:rgb(var(--theme-text)/.03); } .workflow-actions { display:flex; gap:.5rem; padding-bottom:.5rem; }
  .message,.more { font-size:.8rem; color:rgb(var(--theme-text-secondary)); padding:1rem; } .more { border-radius:.4rem; background:rgb(var(--theme-text)/.04); margin:.75rem 0; }
  .discussion { max-width:52rem; margin:auto; } .discussion article { padding:1rem 0; border-bottom:1px solid rgb(var(--theme-border)/.5); } .discussion header { padding:0 0 .6rem; font-size:.8rem; } .discussion header a { margin-left:auto; } .checks { max-width:52rem; margin:auto; } .checks strong { flex:1; font-weight:500; }
  aside section { border-bottom:1px solid rgb(var(--theme-border)/.35); padding-bottom:1rem; margin-bottom:1rem; }
  aside section:last-child { border:0; }
  aside h2 { font-size:.6875rem; }
  .change-count,.check-preview,.merge-status { display:flex; align-items:flex-start; gap:.4rem; line-height:1.5; }
  .change-count { align-items:center; flex-wrap:wrap; }
  .change-total,.change-stats { display:inline-flex; align-items:center; gap:.4rem; }
  .change-stats { font-variant-numeric:tabular-nums; }
  .review-symbol { display:inline-flex; align-items:center; justify-content:center; flex:0 0 1.125rem; width:1.125rem; height:1.125rem; }
  .review-symbol :global(svg) { display:block; flex-shrink:0; }
  .check-copy,.merge-status > span { min-width:0; overflow-wrap:anywhere; }
  .check-preview small { display:block; margin-top:.15rem; font-size:.625rem; line-height:1.5; color:rgb(var(--theme-text-tertiary)); }
  .passed { color:rgb(var(--theme-success)); } .failed { color:rgb(var(--theme-error)); }
  .text-action { display:inline-flex; align-items:center; gap:.3rem; padding:0; margin-top:.25rem; font-size:.6875rem; text-decoration:none; color:rgb(var(--theme-link)); }
  .merge-button,.comment-entry { border:1px solid rgb(var(--theme-border)/.6); border-radius:.4rem; padding:.4rem .65rem; font-size:.75rem; }
  .comment-entry { margin-top:1.75rem; width:100%; text-align:left; color:rgb(var(--theme-text-tertiary)); }
  aside details { margin-top:.75rem; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); } aside summary { cursor:pointer; } aside details p,aside code { margin-top:.5rem; font-size:.6875rem; overflow-wrap:anywhere; }
  .diff-options { margin-left:auto; }
  .file-row { min-height:2.3rem; }
  .file-row :global(svg) { flex-shrink:0; color:rgb(var(--theme-text-tertiary)); }
  .file-row :global(.file-chevron-open) { transform:rotate(90deg); }
  .file-row:hover { background:rgb(var(--theme-text)/.025); }
  .file-row small { color:rgb(var(--theme-text-tertiary)); }
  .lens-heading--changes :global(.review-reading-heading) { margin-bottom:1rem; }
  .lens-heading--changes :global(h1) { font-size:1.1rem; }
  .lens-heading--changes :global(.byline) { margin-top:.4rem; }
  .lens-heading { max-width:64rem; margin:0 auto; }
  .lens-heading.lens-heading--changes { max-width:none; }
  .description { font-size:.8125rem; line-height:1.65; overflow-wrap:anywhere; }
  @container(max-width:700px) { .summary { grid-template-columns:minmax(0,1fr); gap:1rem; } aside { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 1.5rem; } }
  @media(max-width:500px) { .review-toolbar { flex-wrap:wrap; } aside { grid-template-columns:minmax(0,1fr); } nav { gap:.8rem; flex-wrap:wrap; } }
</style>
