<script lang="ts">
  import {
    ChevronRight,
    Copy,
    GitCommitHorizontal,
    GitPullRequestArrow,
    History,
    Save,
    ScanSearch,
    UserRound,
  } from "@lucide/svelte";
  import type { ProviderHandoff, ReviewProjection, WorldAvecResult } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    providerHandoff?: ProviderHandoff | null;
    worldInsight?: WorldAvecResult | null;
    baseRef?: string | null;
    onExport?: () => void;
    onOpenHistory?: () => void;
    onOpenPullRequest?: () => void;
    onShare?: () => void;
  }

  let {
    review,
    providerHandoff = null,
    worldInsight = null,
    baseRef = null,
    onExport,
    onOpenHistory,
    onOpenPullRequest,
    onShare,
  }: Props = $props();

  let copied = $state(false);

  const writtenBy = $derived.by(() => {
    const labels = review.attribution.map((source) => source.label).filter(Boolean);
    const attempt =
      review.attempt_seq != null && review.candidates.length > 1
        ? `attempt ${review.attempt_seq} of ${review.candidates.length}`
        : review.attempt_seq != null
          ? `attempt ${review.attempt_seq}`
          : null;
    if (labels.length === 0 && !attempt) return null;
    if (labels.length === 0) return attempt;
    if (!attempt) return labels.join(", ");
    return `${labels.join(", ")} · ${attempt}`;
  });

  const baseCommit = $derived.by(() => {
    const oid = review.baseline_oid?.slice(0, 7);
    const branch =
      review.candidates.find((candidate) => candidate.attempt_id === review.attempt_id)?.branch
      ?? baseRef
      ?? null;
    if (!oid && !branch) return null;
    if (branch && oid) return `${branch} @ ${oid}`;
    return branch ?? oid ?? null;
  });

  const historyCount = $derived(review.timeline.length);

  const patchRecord = $derived(
    review.evidence_digest ? review.evidence_digest.slice(0, 12) : null,
  );

  const pullRequest = $derived.by(() => {
    if (!providerHandoff) return null;
    if (!providerHandoff.available && !providerHandoff.review_url) return null;
    if (providerHandoff.review_url) {
      return providerHandoff.repository || "Open review";
    }
    if (providerHandoff.available) {
      return providerHandoff.repository || "Ready to share";
    }
    return null;
  });

  const indexCoverage = $derived.by(() => {
    const avec = worldInsight?.code_avec;
    if (!avec || avec.scoreable_entities <= 0) return null;
    return `${avec.fully_scored_entities} of ${avec.scoreable_entities}`;
  });

  async function copyDigest() {
    if (!review.evidence_digest) return;
    try {
      await navigator.clipboard.writeText(review.evidence_digest);
      copied = true;
      window.setTimeout(() => {
        copied = false;
      }, 1400);
    } catch {
      /* ignore */
    }
  }
</script>

<section class="provenance" aria-label="Review provenance">
  {#if writtenBy}
    <div class="provenance-tile">
      <span class="provenance-icon"><UserRound size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">Written by</span>
        <span class="provenance-value">{writtenBy}</span>
      </div>
    </div>
  {/if}

  {#if baseCommit}
    <div class="provenance-tile">
      <span class="provenance-icon"><GitCommitHorizontal size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">Base commit</span>
        <span class="provenance-value provenance-value--mono">
          {baseCommit}
          {#if review.base_advanced}
            <small>base moved</small>
          {/if}
        </span>
      </div>
      {#if onExport}
        <button type="button" class="provenance-action" onclick={() => onExport()}>
          <Save size={12} />
          <span>Export patch…</span>
        </button>
      {/if}
    </div>
  {/if}

  {#if historyCount > 0}
    <button
      type="button"
      class="provenance-tile provenance-tile--button"
      onclick={() => onOpenHistory?.()}
    >
      <span class="provenance-icon"><History size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">History</span>
        <span class="provenance-value"
          >{historyCount} {historyCount === 1 ? "event" : "events"}</span
        >
      </div>
      <ChevronRight size={13} class="provenance-chevron" />
    </button>
  {/if}

  {#if patchRecord}
    <div class="provenance-tile">
      <span class="provenance-icon"><ScanSearch size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">Patch record</span>
        <span class="provenance-value provenance-value--mono">{patchRecord}…</span>
      </div>
      <button
        type="button"
        class="provenance-action"
        title={copied ? "Copied" : "Copy digest"}
        onclick={() => void copyDigest()}
      >
        <Copy size={12} />
        <span>{copied ? "Copied" : "Copy"}</span>
      </button>
    </div>
  {/if}

  {#if pullRequest}
    <div class="provenance-tile">
      <span class="provenance-icon"><GitPullRequestArrow size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">Pull request</span>
        <span class="provenance-value">{pullRequest}</span>
      </div>
      {#if providerHandoff?.review_url}
        <button
          type="button"
          class="provenance-action"
          onclick={() =>
            window.open(providerHandoff?.review_url ?? "", "_blank", "noopener,noreferrer")}
        >Open</button>
      {:else if providerHandoff?.available && onShare}
        <button type="button" class="provenance-action" onclick={() => onShare()}>Share…</button>
      {:else if onOpenPullRequest}
        <button type="button" class="provenance-action" onclick={() => onOpenPullRequest()}
          >Details</button
        >
      {/if}
    </div>
  {/if}

  {#if indexCoverage}
    <div class="provenance-tile">
      <span class="provenance-icon"><ScanSearch size={13} strokeWidth={1.8} /></span>
      <div class="provenance-copy">
        <span class="provenance-label">Index coverage</span>
        <span class="provenance-value">{indexCoverage}</span>
      </div>
    </div>
  {/if}
</section>

<style>
  .provenance {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11.5rem, 1fr));
    gap: 0.45rem;
    margin: 0 0 0.85rem;
  }

  .provenance-tile,
  .provenance-tile--button {
    display: grid;
    grid-template-columns: 1.5rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.45rem;
    min-height: 2.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-900) / 0.35);
    padding: 0.45rem 0.55rem;
    text-align: left;
    color: inherit;
  }

  .provenance-tile--button {
    cursor: pointer;
  }

  .provenance-tile--button:hover {
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .provenance-icon {
    display: inline-flex;
    width: 1.5rem;
    height: 1.5rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.35rem;
    background: rgb(var(--color-surface-800) / 0.45);
    color: rgb(var(--theme-text-quiet));
  }

  .provenance-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
  }

  .provenance-label {
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-faint));
  }

  .provenance-value {
    overflow: hidden;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.3;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
  }

  .provenance-value--mono {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 500;
  }

  .provenance-value small {
    margin-left: 0.35rem;
    font-family: inherit;
    font-size: 0.625rem;
    font-weight: 500;
    color: rgb(var(--theme-warning));
    text-transform: lowercase;
  }

  .provenance-action {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.25rem 0.4rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  .provenance-action:hover {
    background: rgb(var(--color-surface-700) / 0.4);
    color: rgb(var(--color-surface-100));
  }

  :global(.provenance-chevron) {
    color: rgb(var(--theme-text-faint));
  }
</style>
