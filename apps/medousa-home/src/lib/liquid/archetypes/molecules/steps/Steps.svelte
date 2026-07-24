<script lang="ts">
  /**
   * `steps` molecule — numbered vertical how-to rail.
   * Paste-first from ```steps markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import { renderInlineMarkdown } from "$lib/markdown";

  type StepStatus = "done" | "current" | "pending";

  interface StepItem {
    id: string;
    label: string;
    body?: string;
    status?: StepStatus;
    emoji?: string;
    icon?: string;
  }

  const STATUSES = new Set<StepStatus>(["done", "current", "pending"]);

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(
    typeof node.props.subtitle === "string" ? node.props.subtitle : "",
  );

  const steps = $derived.by((): StepItem[] => {
    const raw = node.props.steps;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `step-${i}`;
        const step: StepItem = { id, label };
        if (typeof row.body === "string" && row.body.trim()) step.body = row.body.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) step.emoji = row.emoji.trim();
        if (typeof row.icon === "string" && row.icon.trim()) step.icon = row.icon.trim();
        const status = typeof row.status === "string" ? row.status.trim().toLowerCase() : "";
        if (STATUSES.has(status as StepStatus)) step.status = status as StepStatus;
        return step;
      })
      .filter((s): s is StepItem => s !== null);
  });

  function selectStep(step: StepItem) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { stepId: step.id, label: step.label }),
    );
  }
</script>

{#if steps.length >= 2}
  <div class="liquid-steps" role="list" aria-label={title || "Steps"}>
    {#if title || subtitle}
      <header class="liquid-steps-header">
        {#if title}
          <h3 class="liquid-steps-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-steps-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
      </header>
    {/if}

    <ol class="liquid-steps-rail">
      {#each steps as step, i (step.id)}
        <li
          class="liquid-steps-item"
          class:liquid-steps-done={step.status === "done"}
          class:liquid-steps-current={step.status === "current"}
          class:liquid-steps-pending={step.status === "pending" || !step.status}
          style="--stagger: {i}"
          role="listitem"
        >
          <div class="liquid-steps-spine" aria-hidden="true">
            <span class="liquid-steps-num">{i + 1}</span>
            {#if i < steps.length - 1}
              <span class="liquid-steps-line"></span>
            {/if}
          </div>
          <button type="button" class="liquid-steps-card" onclick={() => selectStep(step)}>
            <span class="liquid-steps-label-row">
              {#if step.emoji || step.icon}
                <span class="liquid-steps-emoji" aria-hidden="true">
                  <LiquidGlyph icon={step.icon} emoji={step.emoji} size={14} />
                </span>
              {/if}
              <span class="liquid-steps-label">{@html renderInlineMarkdown(step.label)}</span>
            </span>
            {#if step.body}
              <span class="liquid-steps-body">{@html renderInlineMarkdown(step.body)}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ol>
  </div>
{/if}

<style>
  .liquid-steps {
    margin: 0;
    padding: 0.75rem 0.85rem 0.9rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    min-width: 0;
  }

  .liquid-steps-header {
    margin-bottom: 0.7rem;
  }

  .liquid-steps-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .liquid-steps-subtitle {
    margin: 0.3rem 0 0;
    font-size: 0.78rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-steps-rail {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .liquid-steps-item {
    display: grid;
    grid-template-columns: 1.75rem 1fr;
    gap: 0.65rem;
    min-width: 0;
  }

  .liquid-steps-spine {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .liquid-steps-num {
    width: 1.55rem;
    height: 1.55rem;
    border-radius: 999px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.7rem;
    font-weight: 700;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-600) 70%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-surface-400) 35%, transparent);
    flex: 0 0 auto;
  }

  .liquid-steps-done .liquid-steps-num {
    background: color-mix(in srgb, var(--color-success-500) 35%, transparent);
    border-color: color-mix(in srgb, var(--color-success-400) 50%, transparent);
  }

  .liquid-steps-current .liquid-steps-num {
    background: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    border-color: color-mix(in srgb, var(--color-primary-400) 55%, transparent);
  }

  .liquid-steps-line {
    flex: 1 1 auto;
    width: 2px;
    min-height: 0.85rem;
    margin: 0.25rem 0;
    background: color-mix(in srgb, var(--color-surface-500) 40%, transparent);
  }

  .liquid-steps-card {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    align-items: flex-start;
    width: 100%;
    margin: 0 0 0.85rem;
    padding: 0.15rem 0 0;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-steps-label-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .liquid-steps-emoji {
    font-size: 0.95rem;
    line-height: 1;
  }

  .liquid-steps-label {
    font-size: 0.86rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .liquid-steps-body {
    font-size: 0.78rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-300));
  }
</style>
