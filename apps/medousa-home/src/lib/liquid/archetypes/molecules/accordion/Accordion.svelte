<script lang="ts">
  import "$lib/styles/liquid-accordion.postcss";
  /**
   * `accordion` molecule — collapsible labeled sections.
   * Paste-first from ```accordion markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import { renderInlineMarkdown, renderMarkdown } from "$lib/markdown/render";
  import { codeBlockControls } from "$lib/markdown/codeBlocks";

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
        <div class="liquid-accordion-item" id={item.id} style="--stagger: {i}">
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
            <div class="liquid-accordion-panel liquid-accordion-body" use:codeBlockControls>
              {@html renderMarkdown(item.body)}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
