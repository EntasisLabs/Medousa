<script lang="ts">
  import { onDestroy } from "svelte";
  import { CircleCheck, CircleAlert, CircleDashed, ChevronRight, ArrowUpRight, GitMerge } from "@lucide/svelte";
  import ReviewTypedChanges from "./ReviewTypedChanges.svelte";
  import ReviewMetadataPicker from "./ReviewMetadataPicker.svelte";
  import { reviewPeople } from "$lib/code/reviewPeople";
  import type { ReviewChangeKind } from "$lib/code/reviewChangeKinds";
  import type { HostedReview, HostedReviewCheck, HostedReviewFile, ReviewPerson, ReviewProviderQuery, ReviewProviderResult } from "$lib/types/generated/daemon_api";
  let { review, files, filesLoading, checks, checksLoaded, providerLabel, operations, connectionId, query, onupdated, onchange, onchecks, onmerge, onlink }: {
    review: HostedReview; files: HostedReviewFile[]; filesLoading: boolean; checks: HostedReviewCheck[]; checksLoaded: boolean;
    providerLabel: string; operations: string[]; connectionId: string | null;
    query: (request: ReviewProviderQuery["request"]) => Promise<ReviewProviderResult>; onupdated: () => Promise<void>;
    onchange: (kind: ReviewChangeKind | null) => void; onchecks: (id?: string) => void; onmerge: () => void; onlink: (event: MouseEvent, url: string) => void;
  } = $props();
  let history = $state<ReviewPerson[]>([]), historyError = $state(""), historyLoading = $state(false), historyPartial = $state(false);
  let alive = true, generation = 0;
  const people = $derived(reviewPeople(history, review.requested_reviewers ?? []));
  const passed = (check: HostedReviewCheck) => (check.conclusion ?? check.status) === "success";
  const failed = (check: HostedReviewCheck) => ["failure", "timed_out", "error", "action_required"].includes(check.conclusion ?? check.status);
  const passCount = $derived(checks.filter(passed).length);
  const stateLabel: Record<string, string> = { pending: "Requested", approved: "Approved", changes_requested: "Changes requested", commented: "Commented", dismissed: "Dismissed" };
  $effect(() => { const currentReview = review; if (operations.includes("reviewers")) void loadPeople(currentReview); });
  onDestroy(() => { alive = false; generation++; });
  async function loadPeople(snapshot: HostedReview) {
    const current = ++generation;
    historyLoading = true; historyError = "";
    let cursor: string | null = null, rows: ReviewPerson[] = [];
    try {
      for (let page = 0; page < 10; page++) {
        const result = await query({ operation: "reviewers", review: snapshot.reference, cursor });
        if (!alive || current !== generation) return;
        if (result.kind !== "people") throw new Error("Review history is unavailable.");
        rows.push(...result.people); cursor = result.next_cursor ?? null;
        if (!cursor) break;
      }
      history = rows; historyPartial = Boolean(cursor);
    } catch { if (alive && current === generation) historyError = "Review history unavailable. Pending requests are shown."; }
    finally { if (alive && current === generation) historyLoading = false; }
  }
</script>
<aside aria-label="Review status">
  <section><ReviewTypedChanges {files} total={review.changed_files} loading={filesLoading} onselect={onchange}/></section>
  {#if operations.includes("labels") || review.labels?.length}
    <section><div class="heading"><h2>Labels</h2>{#if operations.includes("labels") && (review.actions?.includes("add_labels") || review.actions?.includes("remove_label"))}<ReviewMetadataPicker kind="labels" {review} {connectionId} {query} {onupdated}/>{/if}</div>
      <div class="labels">{#each review.labels ?? [] as label (label.id)}<span class="label" title={label.description}><span class="dot" style:background-color={/^[\da-f]{6}$/i.test(label.color ?? "") ? `#${label.color}` : undefined}></span>{label.name}</span>{:else}<p class="quiet">No labels</p>{/each}</div>
    </section>
  {/if}
  <section><div class="heading"><h2>Reviewers</h2>{#if operations.includes("reviewer_candidates") && review.actions?.includes("request_reviewers")}<ReviewMetadataPicker kind="reviewers" {review} {connectionId} {query} {onupdated}/>{/if}</div>
    {#each people as person (person.id)}<div class="person"><span class="avatar">{person.name.slice(0, 2).toUpperCase()}</span><span class="person-name">{person.name}</span><span class="person-state" class:success={person.state === "approved"} class:failure={person.state === "changes_requested"} title={stateLabel[person.state ?? ""] ?? "Reviewed"} aria-label={stateLabel[person.state ?? ""] ?? "Reviewed"}>{#if person.state === "approved"}<CircleCheck size={14}/>{:else if person.state === "changes_requested"}<CircleAlert size={14}/>{:else}<span>{stateLabel[person.state ?? ""] ?? "Reviewed"}</span>{/if}</span></div>{:else}<p class="quiet">{historyLoading ? "Loading reviewers…" : "No reviewers"}</p>{/each}
    {#if historyError}<p class="quiet">{historyError}</p>{:else if historyPartial}<p class="quiet">Showing loaded review history</p>{/if}
  </section>
  <section><div class="heading"><h2>Checks</h2>{#if checks.length}<span class="quiet">{passCount}/{checks.length} passed{checksLoaded ? "" : " · loading"}</span>{/if}</div>
    {#each checks.slice(0, 5) as check (check.id)}<button type="button" class="check-row" onclick={() => onchecks(check.actions?.includes("details") ? check.id : undefined)} title={(check.conclusion ?? check.status).replaceAll("_", " ")}><span class="symbol" class:success={passed(check)} class:failure={failed(check)}>{#if passed(check)}<CircleCheck size={14}/>{:else if failed(check)}<CircleAlert size={14}/>{:else}<CircleDashed size={14}/>{/if}<span class="sr-only">{(check.conclusion ?? check.status).replaceAll("_", " ")}</span></span><span class="check-name">{check.name}</span><ChevronRight size={12}/></button>{:else}<p class="quiet">{checksLoaded ? "No checks recorded" : "Checks unavailable or loading"}</p>{/each}
    {#if checks.length}<button type="button" class="text-action" onclick={() => onchecks()}>View all checks{checks.length > 5 ? ` (${checks.length})` : ""}</button>{/if}
  </section>
  <section><h2>Merge status</h2><p class="merge-status"><span class="symbol" class:success={review.merge_conflicts === false && review.state === "open"} class:failure={review.merge_conflicts === true}>{#if review.state === "merged"}<GitMerge size={14}/>{:else if review.merge_conflicts === false}<CircleCheck size={14}/>{:else if review.merge_conflicts === true}<CircleAlert size={14}/>{:else}<CircleDashed size={14}/>{/if}</span><span>{review.state === "merged" ? "Merged" : review.state === "closed" ? "Closed" : review.state === "draft" ? "Draft review" : review.merge_conflicts === false ? `No conflicts with ${review.base_branch}` : review.merge_conflicts ? "Merge conflicts" : "Conflict status unavailable"}</span></p>
    {#if review.actions?.includes("merge")}<button type="button" class="merge-button" disabled={review.merge_conflicts === true} onclick={onmerge}><GitMerge size={14}/>Merge…</button><p class="quiet merge-hint">Checks and branch requirements still apply.</p>{/if}
  </section>
  <details><summary>Branch &amp; repository</summary><p class="branch">{review.head_branch} → {review.base_branch}</p><code>{review.head_revision.slice(0, 12)}</code><p class="repository">{review.reference.repository}</p><a href={review.url} onclick={(event) => onlink(event, review.url)} onauxclick={(event) => { if (event.button === 1) onlink(event, review.url); }} target="_blank" rel="noopener noreferrer">Open on {providerLabel}<ArrowUpRight size={12}/></a></details>
</aside>
<style>
  aside { min-width:0; font-size:.75rem; color:rgb(var(--theme-text)); }
  section { border-bottom:1px solid rgb(var(--theme-border)/.3); padding-bottom:1rem; margin-bottom:1rem; }
  h2 { margin:0; font-size:.6875rem; font-weight:500; color:rgb(var(--theme-text-tertiary)); } .heading { display:flex; align-items:center; justify-content:space-between; gap:.5rem; min-height:1.4rem; margin-bottom:.4rem; } section > h2 { margin-bottom:.6rem; }
  p { margin:.35rem 0; line-height:1.5; } .quiet { color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; line-height:1.5; }
  button,a { color:inherit; } button { border:0; background:transparent; cursor:pointer; padding:0; } button:focus-visible,a:focus-visible { outline:2px solid rgb(var(--theme-link)); outline-offset:3px; }
  .labels { display:flex; align-items:center; flex-wrap:wrap; gap:.35rem; } .label { display:inline-flex; align-items:center; gap:.35rem; padding:.2rem .45rem; border:1px solid rgb(var(--theme-border)/.4); border-radius:1rem; font-size:.6875rem; overflow-wrap:anywhere; }
  .dot { width:.4rem; height:.4rem; border-radius:50%; flex-shrink:0; background:rgb(var(--theme-text-tertiary)); }
  .person,.check-row,.merge-status { display:flex; align-items:center; gap:.5rem; min-height:1.85rem; line-height:1.5; } .person-name { flex:1; min-width:0; overflow-wrap:anywhere; } .avatar { display:grid; place-items:center; flex:0 0 1.25rem; height:1.25rem; border-radius:50%; background:rgb(var(--theme-text)/.065); color:rgb(var(--theme-text-secondary)); font-size:.5rem; }
  .person-state { display:flex; align-items:center; color:rgb(var(--theme-text-tertiary)); font-size:.625rem; } .symbol { display:flex; align-items:center; justify-content:center; flex:0 0 1.125rem; height:1.125rem; color:rgb(var(--theme-text-tertiary)); }
  .success { color:rgb(var(--theme-success)); } .failure { color:rgb(var(--theme-error)); } .check-row { width:100%; text-align:left; font-size:.75rem; } .check-name { flex:1; min-width:0; overflow-wrap:anywhere; } .check-row > :global(svg) { flex-shrink:0; color:rgb(var(--theme-text-tertiary)); } .check-row:hover .check-name { color:rgb(var(--theme-link)); }
  .text-action,details a { display:inline-flex; align-items:center; gap:.3rem; margin-top:.5rem; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); text-decoration:none; }
  .merge-status { align-items:flex-start; } .merge-status > span:last-child { min-width:0; overflow-wrap:anywhere; } .merge-button { display:flex; justify-content:center; align-items:center; gap:.45rem; width:100%; padding:.55rem .7rem; margin-top:.7rem; border-radius:.4rem; background:rgb(var(--theme-text)); color:rgb(var(--theme-canvas)); font-size:.75rem; font-weight:550; } .merge-button:disabled { opacity:.45; cursor:default; } .merge-hint { font-size:.625rem; margin-top:.5rem; }
  details { font-size:.6875rem; color:rgb(var(--theme-text-tertiary)); } summary { cursor:pointer; } .branch,.repository,code { display:block; margin-top:.65rem; font-size:.6875rem; overflow-wrap:anywhere; } code { font-size:.625rem; }
  @container(max-width:700px) { aside { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 1.5rem; } }
  @media(max-width:500px) { aside { grid-template-columns:minmax(0,1fr); } }
</style>
