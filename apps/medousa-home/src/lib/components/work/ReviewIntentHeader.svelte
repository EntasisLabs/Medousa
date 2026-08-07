<script lang="ts">
  import type { ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    followUp?: boolean;
  }

  let { review, followUp = false }: Props = $props();

  const outcome = $derived.by(() => {
    const brief = review.synthesis.outcome?.trim();
    if (brief) return brief;
    const intents = review.changed_files
      .flatMap((file) => file.intents ?? [])
      .filter(Boolean);
    const unique = [...new Set(intents)];
    if (unique.length === 1) return unique[0]!;
    if (unique.length > 1) {
      return `${unique.length} edits across ${review.changed_files.length} ${review.changed_files.length === 1 ? "file" : "files"}`;
    }
    const n = review.changed_files.length;
    if (n === 0) return "No file changes in this revision";
    return `Changes in ${n} ${n === 1 ? "file" : "files"}`;
  });

  const chips = $derived.by(() => {
    const out: Array<{ id: string; label: string }> = [];
    for (const source of review.attribution) {
      if (!source.label) continue;
      out.push({ id: source.id, label: source.label });
    }
    if (review.synthesis.verification?.success) {
      out.push({ id: "checked", label: "Checked" });
    }
    if (followUp) {
      out.push({ id: "follow-up", label: "Follow-up to your last review" });
    }
    return out;
  });

  const risk = $derived(review.synthesis.risk);
</script>

<header class="intent-header" aria-label="Review summary">
  <p class="intent-outcome">{outcome}</p>
  <div class="intent-meta">
    {#if chips.length}
      <ul class="intent-chips">
        {#each chips as chip (chip.id)}
          <li>{chip.label}</li>
        {/each}
      </ul>
    {/if}
    <span
      class="intent-risk intent-risk--{risk}"
      title={review.synthesis.risk_summary}
    >{risk} risk</span>
  </div>
</header>

<style>
  .intent-header {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    margin-bottom: 0.85rem;
  }

  .intent-outcome {
    margin: 0;
    max-width: 42rem;
    font-size: 0.95rem;
    font-weight: 550;
    line-height: 1.45;
    color: var(--color-content-primary, var(--syn-fg, #e8e6e3));
    letter-spacing: -0.01em;
  }

  .intent-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem 0.65rem;
  }

  .intent-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .intent-chips li {
    padding: 0.12rem 0.45rem;
    border-radius: 999px;
    border: 1px solid color-mix(in oklab, var(--syn-border, #3a3a3a) 80%, transparent);
    background: color-mix(in oklab, var(--syn-bg-elevated, #1c1c1c) 70%, transparent);
    font-size: 0.68rem;
    font-weight: 500;
    color: var(--color-content-secondary, #b8b4ae);
  }

  .intent-risk {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: lowercase;
    color: var(--color-content-quiet, #8a8580);
  }

  .intent-risk--attention,
  .intent-risk--high {
    color: var(--color-warning-600, #c9893a);
  }
</style>
