<script lang="ts">
  /**
   * `decision` organism — options with pros/cons and a recommended pick.
   * Paste-first from ```decision markdown. Complements compare (matrix) and shortlist (rank).
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  interface DecisionOption {
    id: string;
    label: string;
    pros: string[];
    cons: string[];
    score?: string;
    summary?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const factors = $derived(typeof node.props.factors === "string" ? node.props.factors.trim() : "");
  const recommendation = $derived(
    typeof node.props.recommendation === "string" ? node.props.recommendation.trim() : "",
  );

  const options = $derived.by((): DecisionOption[] => {
    const raw = node.props.options;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `option-${i}`;
        const pros = Array.isArray(row.pros)
          ? row.pros.filter((p): p is string => typeof p === "string" && p.trim().length > 0)
          : [];
        const cons = Array.isArray(row.cons)
          ? row.cons.filter((c): c is string => typeof c === "string" && c.trim().length > 0)
          : [];
        const opt: DecisionOption = { id, label, pros, cons };
        if (typeof row.score === "string" && row.score.trim()) opt.score = row.score.trim();
        if (typeof row.summary === "string" && row.summary.trim()) opt.summary = row.summary.trim();
        return opt;
      })
      .filter((o): o is DecisionOption => o !== null);
  });

  const factorsParts = $derived(
    factors
      ? factors
          .split(/[·|,/]/)
          .map((p) => p.trim())
          .filter(Boolean)
      : [],
  );

  function isRecommended(opt: DecisionOption): boolean {
    if (!recommendation) return false;
    return opt.label.trim().toLowerCase() === recommendation.toLowerCase();
  }

  function selectOption(opt: DecisionOption) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", {
        optionId: opt.id,
        label: opt.label,
        recommended: isRecommended(opt),
      }),
    );
  }
</script>

{#if options.length >= 2}
  <div class="liquid-decision" role="list" aria-label={title || "Decision"}>
    {#if title || subtitle || factors || recommendation}
      <header class="liquid-decision-header">
        {#if title}
          <h3 class="liquid-decision-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-decision-subtitle">{subtitle}</p>
        {/if}
        {#if factorsParts.length}
          <div class="liquid-decision-factors">
            {#each factorsParts as part, i (i)}
              <span class="liquid-decision-factor">{part}</span>
            {/each}
          </div>
        {/if}
        {#if recommendation}
          <p class="liquid-decision-rec-banner">
            Recommended · <strong>{recommendation}</strong>
          </p>
        {/if}
      </header>
    {/if}

    <div class="liquid-decision-options">
      {#each options as opt (opt.id)}
        <div
          class="liquid-decision-option"
          class:liquid-decision-option-rec={isRecommended(opt)}
          role="listitem"
        >
          <button type="button" class="liquid-decision-option-btn" onclick={() => selectOption(opt)}>
            <span class="liquid-decision-option-head">
              <span class="liquid-decision-option-label">{opt.label}</span>
              {#if isRecommended(opt)}
                <span class="liquid-decision-option-badge">Recommended</span>
              {/if}
              {#if opt.score}
                <span class="liquid-decision-option-score">{opt.score}</span>
              {/if}
            </span>
            {#if opt.summary}
              <span class="liquid-decision-option-summary">{opt.summary}</span>
            {/if}
            {#if opt.pros.length || opt.cons.length}
              <span class="liquid-decision-tradeoffs">
                {#if opt.pros.length}
                  <span class="liquid-decision-col liquid-decision-pros">
                    <span class="liquid-decision-col-label">Pros</span>
                    <ul>
                      {#each opt.pros as pro, i (i)}
                        <li>{pro}</li>
                      {/each}
                    </ul>
                  </span>
                {/if}
                {#if opt.cons.length}
                  <span class="liquid-decision-col liquid-decision-cons">
                    <span class="liquid-decision-col-label">Cons</span>
                    <ul>
                      {#each opt.cons as con, i (i)}
                        <li>{con}</li>
                      {/each}
                    </ul>
                  </span>
                {/if}
              </span>
            {/if}
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .liquid-decision {
    margin: 0;
    padding: 0.85rem 0.9rem 0.95rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-decision-header {
    margin-bottom: 0.75rem;
  }

  .liquid-decision-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-decision-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-decision-factors {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.5rem;
  }

  .liquid-decision-factor {
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    color: rgb(var(--color-surface-300));
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .liquid-decision-rec-banner {
    margin: 0.55rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-primary-200));
  }

  .liquid-decision-rec-banner strong {
    font-weight: 700;
    color: rgb(var(--color-primary-100));
  }

  .liquid-decision-options {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  .liquid-decision-option {
    border-radius: 0.7rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 40%, transparent);
    overflow: hidden;
  }

  .liquid-decision-option-rec {
    border-color: color-mix(in srgb, var(--color-primary-400) 45%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
  }

  .liquid-decision-option-btn {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.4rem;
    width: 100%;
    margin: 0;
    padding: 0.7rem 0.75rem;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-decision-option-btn:hover {
    background: color-mix(in srgb, var(--color-surface-50) 5%, transparent);
  }

  .liquid-decision-option-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 0.55rem;
  }

  .liquid-decision-option-label {
    font-size: 0.92rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-decision-option-badge {
    font-size: 0.58rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: rgb(var(--color-primary-300));
  }

  .liquid-decision-option-score {
    margin-left: auto;
    font-size: 0.85rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-primary-300));
  }

  .liquid-decision-option-summary {
    font-size: 0.75rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-300));
  }

  .liquid-decision-tradeoffs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.55rem;
    margin-top: 0.15rem;
  }

  @media (max-width: 420px) {
    .liquid-decision-tradeoffs {
      grid-template-columns: 1fr;
    }
  }

  .liquid-decision-col-label {
    display: block;
    font-size: 0.58rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    margin-bottom: 0.25rem;
  }

  .liquid-decision-pros .liquid-decision-col-label {
    color: rgb(var(--color-success-300));
  }

  .liquid-decision-cons .liquid-decision-col-label {
    color: rgb(var(--color-warning-300));
  }

  .liquid-decision-col ul {
    margin: 0;
    padding: 0 0 0 0.9rem;
  }

  .liquid-decision-col li {
    font-size: 0.72rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-200));
    margin: 0.15rem 0;
  }
</style>
