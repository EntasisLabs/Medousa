<script lang="ts">
  import { Copy, Save } from "@lucide/svelte";
  import type { ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    baseRef?: string | null;
    onExport?: () => void;
  }

  let {
    review,
    baseRef = null,
    onExport,
  }: Props = $props();

  let copied = $state(false);

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

  const patchRecord = $derived(
    review.evidence_digest ? review.evidence_digest.slice(0, 12) : null,
  );

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

<dl class="custody" aria-label="Review custody">
  {#if baseCommit}
    <div class="custody-row">
      <dt>Base</dt>
      <dd class="custody-mono">
        {baseCommit}
        {#if review.base_advanced}
          <span class="custody-warn">moved</span>
        {/if}
      </dd>
      {#if onExport}
        <button type="button" class="custody-action" onclick={() => onExport()}>
          <Save size={12} />
          <span>Export…</span>
        </button>
      {/if}
    </div>
  {/if}

  {#if patchRecord}
    <div class="custody-row">
      <dt>Digest</dt>
      <dd class="custody-mono">{patchRecord}…</dd>
      <button
        type="button"
        class="custody-action"
        title={copied ? "Copied" : "Copy digest"}
        onclick={() => void copyDigest()}
      >
        <Copy size={12} />
        <span>{copied ? "Copied" : "Copy"}</span>
      </button>
    </div>
  {/if}
</dl>

<style>
  .custody {
    display: flex;
    flex-direction: column;
    gap: 0;
    margin: 0 0 0.65rem;
    border: 1px solid rgb(var(--theme-border) / 0.22);
    border-radius: var(--theme-control-radius, 0.45rem);
    background: rgb(var(--color-surface-800) / 0.12);
  }

  .custody-row {
    display: grid;
    grid-template-columns: 5.5rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
    min-height: 1.85rem;
    padding: 0.35rem 0.65rem;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.14);
  }

  .custody-row:last-child {
    border-bottom: 0;
  }

  .custody-row dt {
    margin: 0;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-faint));
  }

  .custody-row dd {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--theme-text));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .custody-mono {
    font-family: var(--font-mono);
    font-size: 0.6875rem;
  }

  .custody-warn {
    margin-left: 0.35rem;
    font-family: inherit;
    font-size: 0.625rem;
    font-weight: 600;
    color: rgb(var(--theme-warning));
    text-transform: lowercase;
  }

  .custody-action {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.2rem 0.35rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
    white-space: nowrap;
    cursor: pointer;
  }

  .custody-action:hover {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--theme-text));
  }
</style>
