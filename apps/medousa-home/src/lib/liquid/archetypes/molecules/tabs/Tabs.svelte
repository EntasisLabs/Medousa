<script lang="ts">
  /**
   * `tabs` molecule — horizontal tab switcher with one visible panel.
   * Paste-first from ```tabs markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown";

  interface TabPanel {
    id: string;
    label: string;
    body: string;
    emoji?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(
    typeof node.props.subtitle === "string" ? node.props.subtitle : "",
  );

  const panels = $derived.by((): TabPanel[] => {
    const raw = node.props.panels;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        const body = typeof row.body === "string" ? row.body.trim() : "";
        if (!label || !body) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `tab-${i}`;
        const panel: TabPanel = { id, label, body };
        if (typeof row.emoji === "string" && row.emoji.trim()) panel.emoji = row.emoji.trim();
        return panel;
      })
      .filter((p): p is TabPanel => p !== null);
  });

  function resolveDefaultIndex(list: TabPanel[]): number {
    const def = typeof node.props.default === "string" ? node.props.default.trim() : "";
    if (!def || list.length === 0) return 0;
    const byId = list.findIndex((p) => p.id === def);
    if (byId >= 0) return byId;
    const byLabel = list.findIndex((p) => p.label.toLowerCase() === def.toLowerCase());
    if (byLabel >= 0) return byLabel;
    const asNum = Number(def);
    if (Number.isFinite(asNum) && asNum >= 1 && asNum <= list.length) return asNum - 1;
    return 0;
  }

  let activeIndex = $state(0);
  let seeded = $state(false);

  $effect(() => {
    const list = panels;
    if (!seeded && list.length > 0) {
      activeIndex = resolveDefaultIndex(list);
      seeded = true;
    } else if (activeIndex >= list.length) {
      activeIndex = Math.max(0, list.length - 1);
    }
  });

  function selectTab(index: number) {
    const panel = panels[index];
    if (!panel) return;
    activeIndex = index;
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { panelId: panel.id, label: panel.label }),
    );
  }

  const active = $derived(panels[activeIndex] ?? null);
</script>

{#if panels.length >= 2}
  <div class="liquid-tabs" role="group" aria-label={title || "Tabs"}>
    {#if title || subtitle}
      <header class="liquid-tabs-header">
        {#if title}
          <h3 class="liquid-tabs-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-tabs-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-tabs-list" role="tablist" data-no-tab-swipe>
      {#each panels as panel, i (panel.id)}
        <button
          type="button"
          class="liquid-tabs-tab"
          class:liquid-tabs-tab-active={i === activeIndex}
          role="tab"
          aria-selected={i === activeIndex}
          style="--stagger: {i}"
          onclick={() => selectTab(i)}
        >
          {#if panel.emoji}
            <span class="liquid-tabs-emoji" aria-hidden="true">{panel.emoji}</span>
          {/if}
          <span>{panel.label}</span>
        </button>
      {/each}
    </div>

    {#if ctx.exportPaper}
      {#each panels as panel, i (panel.id)}
        <div
          class="liquid-tabs-panel liquid-tabs-panel--export"
          role="tabpanel"
          style="--stagger: {i}"
        >
          <p class="liquid-tabs-export-label">{panel.label}</p>
          {@html renderInlineMarkdown(panel.body)}
        </div>
      {/each}
    {:else if active}
      <div
        class="liquid-tabs-panel"
        role="tabpanel"
        style="--stagger: {activeIndex}"
      >
        {@html renderInlineMarkdown(active.body)}
      </div>
    {/if}
  </div>
{/if}

<style>
  .liquid-tabs {
    margin: 0;
    padding: 0.75rem 0.85rem 0.9rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    min-width: 0;
  }

  .liquid-tabs-header {
    margin-bottom: 0.65rem;
  }

  .liquid-tabs-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .liquid-tabs-subtitle {
    margin: 0.3rem 0 0;
    font-size: 0.78rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-tabs-list {
    display: flex;
    gap: 0.3rem;
    overflow-x: auto;
    padding-bottom: 0.55rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 24%, transparent);
    -webkit-overflow-scrolling: touch;
  }

  .liquid-tabs-tab {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.35rem 0.65rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: rgb(var(--color-surface-400));
    font-size: 0.78rem;
    font-weight: 550;
    cursor: pointer;
    white-space: nowrap;
  }

  .liquid-tabs-tab-active {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .liquid-tabs-emoji {
    font-size: 0.9rem;
    line-height: 1;
  }

  .liquid-tabs-panel {
    margin-top: 0.7rem;
    font-size: 0.82rem;
    line-height: 1.55;
    color: rgb(var(--color-surface-200));
  }

  .liquid-tabs-panel--export {
    margin-top: 0.85rem;
    padding-top: 0.65rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .liquid-tabs-panel--export:first-of-type {
    border-top: 0;
    padding-top: 0;
  }

  .liquid-tabs-export-label {
    margin: 0 0 0.35rem;
    font-size: 0.72rem;
    font-weight: 650;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
  }
</style>
