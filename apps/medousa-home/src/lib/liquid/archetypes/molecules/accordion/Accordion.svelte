<script lang="ts">
  /**
   * `accordion` molecule — collapsible labeled sections.
   * Paste-first from ```accordion markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import { renderInlineMarkdown } from "$lib/markdown";

  interface AccordionItem {
    id: string;
    label: string;
    body: string;
    open?: boolean;
    emoji?: string;
    icon?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(
    typeof node.props.subtitle === "string" ? node.props.subtitle : "",
  );
  const multiple = $derived(node.props.multiple === true);

  const items = $derived.by((): AccordionItem[] => {
    const raw = node.props.items;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        const body = typeof row.body === "string" ? row.body.trim() : "";
        if (!label || !body) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `item-${i}`;
        const out: AccordionItem = { id, label, body };
        if (row.open === true) out.open = true;
        if (typeof row.emoji === "string" && row.emoji.trim()) out.emoji = row.emoji.trim();
        if (typeof row.icon === "string" && row.icon.trim()) out.icon = row.icon.trim();
        return out;
      })
      .filter((i): i is AccordionItem => i !== null);
  });

  let openIds = $state<Set<string>>(new Set());
  let seeded = $state(false);

  $effect(() => {
    const list = items;
    if (list.length === 0) return;
    // Export capture: mount every panel so FAQ answers are not missing.
    if (ctx.exportPaper) {
      openIds = new Set(list.map((item) => item.id));
      seeded = true;
      return;
    }
    if (!seeded) {
      const initial = new Set<string>();
      for (const item of list) {
        if (item.open) initial.add(item.id);
      }
      if (initial.size === 0 && list[0]) initial.add(list[0].id);
      if (!multiple && initial.size > 1) {
        const first = [...initial][0];
        openIds = new Set(first ? [first] : []);
      } else {
        openIds = initial;
      }
      seeded = true;
    }
  });

  function isOpen(id: string): boolean {
    return openIds.has(id);
  }

  function toggle(item: AccordionItem) {
    const next = new Set(openIds);
    if (next.has(item.id)) {
      next.delete(item.id);
    } else {
      if (!multiple) next.clear();
      next.add(item.id);
    }
    openIds = next;
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", {
        itemId: item.id,
        label: item.label,
        open: next.has(item.id),
      }),
    );
  }
</script>

{#if items.length >= 1}
  <div class="liquid-accordion" role="group" aria-label={title || "Accordion"}>
    {#if title || subtitle}
      <header class="liquid-accordion-header">
        {#if title}
          <h3 class="liquid-accordion-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-accordion-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-accordion-list">
      {#each items as item, i (item.id)}
        <div class="liquid-accordion-item" style="--stagger: {i}">
          <button
            type="button"
            class="liquid-accordion-trigger"
            class:liquid-accordion-open={isOpen(item.id)}
            aria-expanded={isOpen(item.id)}
            onclick={() => toggle(item)}
          >
            <span class="liquid-accordion-label-row">
              {#if item.emoji || item.icon}
                <span class="liquid-accordion-emoji" aria-hidden="true">
                  <LiquidGlyph icon={item.icon} emoji={item.emoji} size={14} />
                </span>
              {/if}
              <span class="liquid-accordion-label">{@html renderInlineMarkdown(item.label)}</span>
            </span>
            <span class="liquid-accordion-chevron" aria-hidden="true">▾</span>
          </button>
          {#if isOpen(item.id)}
            <div class="liquid-accordion-panel">
              {@html renderInlineMarkdown(item.body)}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .liquid-accordion {
    margin: 0;
    padding: 0.75rem 0.85rem 0.9rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    min-width: 0;
  }

  .liquid-accordion-header {
    margin-bottom: 0.65rem;
  }

  .liquid-accordion-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .liquid-accordion-subtitle {
    margin: 0.3rem 0 0;
    font-size: 0.78rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-accordion-list {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .liquid-accordion-item {
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 40%, transparent);
    overflow: hidden;
  }

  .liquid-accordion-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    width: 100%;
    margin: 0;
    padding: 0.55rem 0.7rem;
    border: 0;
    background: transparent;
    color: rgb(var(--color-surface-100));
    text-align: left;
    cursor: pointer;
  }

  .liquid-accordion-label-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .liquid-accordion-emoji {
    font-size: 0.95rem;
    line-height: 1;
  }

  .liquid-accordion-label {
    font-size: 0.82rem;
    font-weight: 600;
  }

  .liquid-accordion-chevron {
    flex: 0 0 auto;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-400));
    transition: transform 0.18s ease;
  }

  .liquid-accordion-open .liquid-accordion-chevron {
    transform: rotate(180deg);
  }

  .liquid-accordion-panel {
    padding: 0 0.7rem 0.65rem;
    font-size: 0.78rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-300));
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-accordion-chevron {
      transition: none;
    }
  }
</style>
