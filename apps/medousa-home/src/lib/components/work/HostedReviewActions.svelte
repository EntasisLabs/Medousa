<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { pushBrowserPopoverOverlay, popBrowserPopoverOverlay } from "$lib/utils/browserPopoverOverlay";
  import { onMount, onDestroy } from "svelte";
  import { reviewDraftKey, readReviewDraft, saveReviewDraft } from "$lib/code/hostedReviewDraft";
  import { mutateHostedReview } from "$lib/code/hostedReviews";
  import type { HostedReview, ReviewInlineComment, ReviewProviderMutation } from "$lib/types/generated/daemon_api";

  let { review, connectionId, scope = "", drafts = [], draftRevision = null, onupdated, onclear }: { review: HostedReview; connectionId: string | null; scope?: string; drafts?: ReviewInlineComment[]; draftRevision?: string | null; onupdated: () => Promise<void>; onclear: () => void } = $props();
  let mode = $state<"" | "comment" | "submit_review" | "edit" | "request_reviewers" | "merge" | "close" | "reopen" | "ready" | "draft">("");
  let body = $state(""), title = $state(""), usernames = $state("");
  let decision = $state<"approve" | "request_changes" | "comment">("comment");
  let method = $state<"merge" | "squash" | "rebase">("squash");
  let busy = $state(false), error = $state<string | null>(null), notice = $state<string | null>(null);
  let alive = true;
  let menuOpen = $state(false);
  let dialog = $state<HTMLDialogElement | null>(null);
  $effect(() => {
    if (!mode || !dialog) return;
    if (!dialog.open) dialog.showModal();
    void pushBrowserPopoverOverlay();
    return () => { dialog?.close(); void popBrowserPopoverOverlay(); };
  });
  let expectedRevision = $state<string | null>(null);
  onMount(() => {
    const saved = readReviewDraft(reviewDraftKey(scope, review.reference, connectionId, "form"));
    if (!saved) return;
    if (["comment", "submit_review", "edit", "request_reviewers", "merge", "close", "reopen", "ready", "draft"].includes(String(saved.mode))) mode = saved.mode as typeof mode;
    for (const field of ["body", "title", "usernames", "revision"]) if (typeof saved[field] !== "string") saved[field] = "";
    body = saved.body as string; title = saved.title as string; usernames = saved.usernames as string; expectedRevision = saved.revision as string;
    if (["approve", "request_changes", "comment"].includes(String(saved.decision))) decision = saved.decision as typeof decision;
    if (["merge", "squash", "rebase"].includes(String(saved.method))) method = saved.method as typeof method;
  });
  onDestroy(() => {
    alive = false;
    saveReviewDraft(reviewDraftKey(scope, review.reference, connectionId, "form"), mode ? { mode, body, title, usernames, decision, method, revision: expectedRevision } : null);
  });
  const staleForm = $derived((mode === "submit_review" || mode === "merge") && expectedRevision !== review.head_revision);
  const staleDrafts = $derived(drafts.length > 0 && draftRevision !== review.head_revision);
  const actions = $derived(review.actions ?? []);
  export function openAction(next: typeof mode) {
    if (busy) return;
    menuOpen = false; mode = next; expectedRevision = review.head_revision; decision = "comment"; error = null; notice = null;
    if (next === "edit") { title = review.title; body = review.body; }
    else body = "";
    if (next === "merge") method = (review.merge_methods?.[0] ?? "merge") as typeof method;
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault(); if (busy || !mode || staleForm || (mode === "submit_review" && staleDrafts)) return;
    let request: ReviewProviderMutation["request"];
    const reference = review.reference;
    if (mode === "comment") request = { operation: "comment", review: reference, body, reply_to: null };
    else if (mode === "submit_review") request = { operation: "submit_review", review: reference, revision: expectedRevision ?? review.head_revision, decision, body, comments: drafts };
    else if (mode === "edit") request = { operation: "edit", review: reference, title, body };
    else if (mode === "request_reviewers") request = { operation: "request_reviewers", review: reference, reviewers: usernames.split(/[\s,]+/).map(v => v.replace(/^@/, "")).filter(Boolean) };
    else if (mode === "merge") request = { operation: "merge", review: reference, revision: expectedRevision ?? review.head_revision, method };
    else request = { operation: "set_state", review: reference, state: mode };
    busy = true; error = null; notice = null;
    try {
      const result = await mutateHostedReview(reference.provider_id, reference.host, request, connectionId);
      if (!alive) return;
      if (result.kind !== "updated") throw new Error("The provider did not confirm the action.");
      if (mode === "submit_review") onclear();
      body = ""; mode = ""; notice = result.message; saveReviewDraft(reviewDraftKey(scope, review.reference, connectionId, "form"), null);
      await onupdated();
    } catch (cause) { error = `${cause instanceof Error ? cause.message : String(cause)} Refresh to check the result before submitting again.`; }
    finally { busy = false; }
  }
</script>

<div class="review-controls">
  <div class="buttons">
    {#if actions.some(action => ["comment", "edit", "request_reviewers", "ready", "draft", "close", "reopen", "merge"].includes(action))}
    <OverflowMenu bind:open={menuOpen} label="Pull request actions" mobileTitle="Pull request actions">
      {#if actions.includes("comment")}<button type="button" role="menuitem" disabled={busy} onclick={() => openAction("comment")}>Comment</button>{/if}
      {#each ["edit", "request_reviewers", "ready", "draft", "close", "reopen"] as action}{#if actions.includes(action)}<button type="button" role="menuitem" disabled={busy} onclick={() => openAction(action as typeof mode)}>{({ edit: "Edit title and description", request_reviewers: "Request reviewers", ready: "Mark ready for review", draft: "Convert to draft", close: "Close pull request", reopen: "Reopen pull request" } as Record<string, string>)[action]}</button>{/if}{/each}
      {#if actions.includes("merge")}<button type="button" role="menuitem" disabled={busy} onclick={() => openAction("merge")}>Merge…</button>{/if}
    </OverflowMenu>
    {/if}
    {#if actions.includes("submit_review")}<button type="button" class="primary" disabled={busy} onclick={() => openAction("submit_review")}>Review{drafts.length ? ` (${drafts.length})` : ""}<ChevronDown size={13} /></button>{/if}
  </div>
  {#if mode}
    <dialog aria-label={mode === "submit_review" ? "Submit review" : "Pull request action"} bind:this={dialog} oncancel={(event) => { event.preventDefault(); if (!busy) mode = ""; }}>
    <form onsubmit={submit} aria-label="Pull request action">
      {#if staleForm}<p class="error">This form was opened for another commit. Cancel and reopen it to review the current revision.</p>{/if}
      {#if mode === "submit_review"}
        <h2>Submit review</h2>{#if staleDrafts}<p class="error">The commit changed after you wrote inline comments. Remove those pending comments before reviewing this revision.</p>{/if}<p>Reviewing commit <code>{review.head_revision.slice(0, 12)}</code>. {drafts.length} inline {drafts.length === 1 ? "comment" : "comments"} included.</p>
        <label>Decision<select aria-label="Review decision" value={decision} onchange={(event) => decision = event.currentTarget.value as typeof decision}><option value="comment">Comment</option>{#if actions.includes("approve")}<option value="approve">Approve</option>{/if}{#if actions.includes("request_changes")}<option value="request_changes">Request changes</option>{/if}</select></label>
      {:else if mode === "merge"}
        <h2>Merge into {review.base_branch}</h2><p>This merges the displayed commit <code>{review.head_revision.slice(0, 12)}</code>. Branch requirements still apply.</p><label>Merge method<select aria-label="Merge method" value={method} onchange={(event) => method = event.currentTarget.value as typeof method}>{#each review.merge_methods ?? [] as option}<option value={option}>{option === "squash" ? "Squash and merge" : option === "rebase" ? "Rebase and merge" : "Merge commit"}</option>{/each}</select></label>
      {:else if mode === "edit"}<h2>Edit pull request</h2><label>Title<input aria-label="Pull request title" bind:value={title} required maxlength={1024} /></label>
      {:else if mode === "request_reviewers"}<h2>Request reviewers</h2><label>Usernames<input aria-label="Reviewer usernames" bind:value={usernames} required placeholder="octocat, teammate" /></label>
      {:else if mode !== "comment"}<h2>{({ close: "Close this pull request?", reopen: "Reopen this pull request?", ready: "Mark ready for review?", draft: "Convert to draft?" } as Record<string, string>)[mode]}</h2>
      {/if}
      {#if mode === "comment" || mode === "submit_review" || mode === "edit"}<label>{mode === "edit" ? "Description" : "Message"}<textarea aria-label={mode === "edit" ? "Pull request description" : "Review message"} bind:value={body} required={mode === "comment" || (mode === "submit_review" && decision !== "approve")} rows={5} maxlength={60000} placeholder="Write in Markdown…"></textarea></label>{/if}
      <div class="buttons"><button class="primary" type="submit" disabled={busy || staleForm || (mode === "submit_review" && staleDrafts)}>{busy ? "Submitting…" : mode === "submit_review" ? "Submit review" : mode === "comment" ? "Post comment" : mode === "merge" ? "Confirm merge" : "Save changes"}</button><button type="button" disabled={busy} onclick={() => mode = ""}>Cancel</button></div>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    </form>
    </dialog>
  {/if}
  {#if error && !mode}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
</div>

<style>
  .review-controls { position:relative; font-size:.75rem; }
  .buttons { display:flex; align-items:center; flex-wrap:wrap; gap:.5rem; }
  button,select,input,textarea { color:rgb(var(--theme-text)); background:rgb(var(--theme-text)/.035); border:1px solid rgb(var(--theme-border)/.6); border-radius:.4rem; font:inherit; padding:.4rem .6rem; }
  button { cursor:pointer; } button:disabled { opacity:.5; cursor:wait; }
  dialog { width:min(30rem,calc(100vw - 2rem)); max-height:calc(100vh - 3rem); padding:1.25rem; border:1px solid rgb(var(--theme-border)/.6); border-radius:.65rem; background:rgb(var(--theme-canvas)); color:rgb(var(--theme-text)); box-shadow:0 1rem 4rem #0005; }
  dialog::backdrop { background:rgb(var(--theme-canvas)/.7); }
  form { margin:0; }
  [role="menuitem"] { display:block; width:100%; text-align:left; border:0; background:transparent; padding:.5rem .6rem; font-size:.75rem; color:rgb(var(--theme-text)); }
  [role="menuitem"]:hover { background:rgb(var(--theme-text)/.06); }
  button.primary { display:inline-flex; align-items:center; gap:.4rem; background:rgb(var(--theme-text)); color:rgb(var(--theme-canvas)); border-color:transparent; }
  h2 { font-size:.85rem; font-weight:550; margin:0 0 .6rem; } p { color:rgb(var(--theme-text-secondary)); line-height:1.6; } .error { color:rgb(var(--color-amber-400)); }
  label { display:flex; flex-direction:column; align-items:flex-start; gap:.4rem; margin:.6rem 0; } input,textarea { width:100%; box-sizing:border-box; } textarea { resize:vertical; min-height:5rem; }
</style>
