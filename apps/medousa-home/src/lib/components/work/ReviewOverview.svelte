<script lang="ts">
  import { ArrowUpRight, CircleCheck, CircleAlert, CircleDashed } from "@lucide/svelte";
  import ReviewPageHeading from "./ReviewPageHeading.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { reviewAreas, reviewFileInDirectory } from "$lib/code/reviewAreas";
  import type { ReviewProjection } from "$lib/forge";

  let { review, originalRequest, sourceBranch, title = "", onShowChanges }: {
    title?: string;
    review: ReviewProjection;
    originalRequest: string;
    sourceBranch: string;
    onShowChanges: (directory: string | null) => void;
  } = $props();

  let requestOpen = $state(false);
  const candidate = $derived(review.candidates.find(candidate => candidate.attempt_id === review.attempt_id));
  const targetLabel = $derived(/^[a-f0-9]{40}$/i.test(sourceBranch) ? sourceBranch.slice(0, 10) : sourceBranch);
  const outcome = $derived(review.synthesis.outcome?.trim() !== originalRequest.trim() ? review.synthesis.outcome?.trim() : "");
  const verification = $derived(review.synthesis.verification);
  const issues = $derived(review.synthesis.unresolved_issues.filter((issue) => !/no project check|project checks haven'?t run/i.test(issue)));
  const additions = $derived(review.changed_files.reduce((count, file) => count + (file.lines_added ?? 0), 0));
  const removals = $derived(review.changed_files.reduce((count, file) => count + (file.lines_removed ?? 0), 0));
  const intents = $derived([...new Set(review.changed_files.flatMap((file) => [file.primary_intent, ...(file.intents ?? [])]).filter((intent): intent is string => Boolean(intent?.trim())))].slice(0, 8));
  const areas = $derived(reviewAreas(review.changed_files).map((area) => ({ ...area, files: review.changed_files.filter((file) => reviewFileInDirectory(file.path, area.directory)) })));
</script>

<article class="review-overview" aria-label="Review overview">
  <div class="overview-reading">
    {#if title}<ReviewPageHeading {title} author={candidate?.executor ?? ""} revision={candidate ? `Revision ${candidate.attempt_seq}` : ""} target={targetLabel} />{/if}
    {#if intents.length || (outcome && outcome !== title)}
      <section class="overview-summary"><h2>Summary</h2>{#if outcome && outcome !== title}<p>{outcome}</p>{/if}{#if intents.length}<ul class="overview-intents">{#each intents as intent}<li>{intent}</li>{/each}</ul>{/if}</section>
    {/if}
    <section>
      <h2>Changed areas</h2>
      {#if !areas.length}<p class="overview-empty">No file changes in this revision.</p>{/if}
      <div class="overview-areas">{#each areas as area}<button type="button" onclick={() => onShowChanges(area.directory)}>
        <span class="area-copy"><strong>{area.label}</strong><span class="area-files">{area.files.slice(0, 3).map((file) => file.path.slice(area.directory ? area.directory.length + 1 : 0)).join(", ")}{#if area.count > 3} <span>+{area.count - 3} more</span>{/if}</span></span>
        <small>{area.count} {area.count === 1 ? "file" : "files"}</small><ArrowUpRight size={13} />
      </button>{/each}</div>
    </section>
    {#if issues.length}<section class="overview-attention"><h2>Needs attention</h2><ul class="overview-intents">{#each issues as issue}<li>{issue}</li>{/each}</ul></section>{/if}
    {#if review.decision?.rationale}<section class="overview-decision"><h2>Review decision</h2><p>{review.decision.rationale}</p></section>{/if}
    {#if originalRequest.trim()}<details bind:open={requestOpen}><summary>Original request</summary>{#if requestOpen}<div class="overview-request"><MarkdownContent presentation="document" content={originalRequest} openLinksInWeb={true} /></div>{/if}</details>{/if}
  </div>
  <aside class="overview-details" aria-label="Revision details">
    <section>
      <h2>Changes</h2>
      <p class="overview-change-count">{review.changed_files.length} {review.changed_files.length === 1 ? "file" : "files"} <span class="overview-stats"><span class="added">+{additions}</span><span class="removed">−{removals}</span></span></p>
      <button type="button" class="overview-view-all" onclick={() => onShowChanges(null)}>View all changes <ArrowUpRight size={12} /></button>
    </section>
    <section aria-label="Recorded checks">
      <h2>Checks</h2>
      <p class="overview-check-status" class:passed={verification?.success} class:failed={verification && !verification.success}>
        <span class="overview-symbol" aria-hidden="true">{#if verification?.success}<CircleCheck size={14} />{:else if verification}<CircleAlert size={14} />{:else}<CircleDashed size={14} />{/if}</span>
        <span>{verification ? (verification.success ? "Check passed" : "Check failed") : "No checks recorded"}</span>
      </p>
      {#if verification}<p class="overview-check-label">{verification.label}{verification.exit_code != null ? ` · exit ${verification.exit_code}` : ""}</p><details class="overview-command"><summary>Command</summary><code>{verification.command.join(" ")}</code></details>{/if}
    </section>
    <section>
      <h2>Target</h2><p class="overview-branch" title={sourceBranch}>{targetLabel}</p>
    </section>
    {#if review.synthesis.risk}<section><h2>Risk</h2><p class="overview-risk" title={review.synthesis.risk_summary}>{review.synthesis.risk === "low" ? "Low" : review.synthesis.risk === "high" ? "High" : review.synthesis.risk === "attention" ? "Needs attention" : review.synthesis.risk}</p></section>{/if}
  </aside>
</article>

<style>
  .review-overview { display:grid; grid-template-columns:minmax(0,1fr) 11rem; align-items:start; gap:2rem; color:rgb(var(--theme-text)); }
  .overview-reading { min-width:0; }
  section { margin:0 0 1.4rem; }
  h2 { font-size:.875rem; font-weight:550; margin:0 0 .65rem; letter-spacing:-.01em; }
  p,li { font-size:.8125rem; line-height:1.65; color:rgb(var(--theme-text-secondary)); }
  p { margin:.25rem 0; }
  .overview-intents { padding-left:1.1rem; margin:.5rem 0; }
  .overview-intents li { padding:.2rem 0; overflow-wrap:anywhere; }
  .overview-areas button { display:flex; align-items:center; width:100%; gap:.75rem; border:0; border-bottom:1px solid rgb(var(--theme-border)/.3); background:transparent; color:rgb(var(--theme-text)); padding:.75rem 0; text-align:left; cursor:pointer; }
  .overview-areas button:first-child { padding-top:.35rem; }
  .area-copy { flex:1; min-width:0; }
  .area-copy strong { display:block; font-size:.8125rem; font-weight:500; }
  .area-files { display:block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-top:.25rem; font-size:.6875rem; line-height:1.5; color:rgb(var(--theme-text-secondary)); }
  .area-files span { margin-left:.35rem; color:rgb(var(--theme-text-tertiary)); }
  .overview-areas small { font-size:.6875rem; color:rgb(var(--theme-text-secondary)); white-space:nowrap; }
  .overview-areas button > :global(svg) { color:rgb(var(--theme-text-tertiary)); flex-shrink:0; }
  .overview-areas button:hover .area-copy strong { color:rgb(var(--theme-link)); }
  .overview-areas button:hover > :global(svg) { color:rgb(var(--theme-text)); }
  .overview-attention { border-left:2px solid rgb(var(--theme-warning)/.6); padding-left:.85rem; }
  .overview-decision { border-left:2px solid rgb(var(--theme-border)); padding-left:.85rem; }
  details { font-size:.75rem; color:rgb(var(--theme-text-secondary)); }
  summary { cursor:pointer; padding:.35rem 0; }
  .overview-request { padding-top:.65rem; font-size:.8125rem; overflow-wrap:anywhere; }
  .overview-details { min-width:0; }
  .overview-details section { border-bottom:1px solid rgb(var(--theme-border)/.35); padding-bottom:1rem; margin-bottom:1rem; }
  .overview-details section:last-child { border:0; }
  .overview-details h2 { font-size:.6875rem; color:rgb(var(--theme-text-tertiary)); font-weight:500; margin-bottom:.5rem; }
  .overview-change-count { display:flex; align-items:center; flex-wrap:wrap; gap:.5rem .75rem; color:rgb(var(--theme-text)); font-size:.8125rem; }
  .overview-stats { display:inline-flex; gap:.4rem; font-size:.6875rem; font-variant-numeric:tabular-nums; }
  .added,.passed { color:rgb(var(--theme-success)); }
  .removed,.failed { color:rgb(var(--theme-error)); }
  .overview-view-all { display:inline-flex; align-items:center; gap:.35rem; margin-top:.4rem; padding:0; border:0; background:transparent; color:rgb(var(--theme-text-secondary)); font-size:.6875rem; cursor:pointer; }
  .overview-view-all:hover { color:rgb(var(--theme-text)); }
  .overview-check-status { display:flex; align-items:flex-start; gap:.4rem; font-size:.75rem; line-height:1.5; }
  .overview-symbol { display:inline-flex; align-items:center; justify-content:center; flex:0 0 1.125rem; width:1.125rem; height:1.125rem; }
  .overview-symbol :global(svg) { display:block; flex-shrink:0; }
  .overview-check-status > span:last-child { min-width:0; overflow-wrap:anywhere; }
  .overview-check-label { font-size:.6875rem; overflow-wrap:anywhere; }
  .overview-branch { font-size:.75rem; overflow-wrap:anywhere; }
  .overview-risk { font-size:.75rem; }
  code { display:block; margin-top:.4rem; overflow-wrap:anywhere; white-space:pre-wrap; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); }
  .overview-command { margin-top:.25rem; font-size:.6875rem; }
  @container (max-width:700px) { .review-overview { grid-template-columns:minmax(0,1fr); gap:1.5rem; } .overview-details { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 1.5rem; } }
</style>
