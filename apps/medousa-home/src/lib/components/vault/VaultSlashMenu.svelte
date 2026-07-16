<script lang="ts">
  import { onMount } from "svelte";
  import {
    SLASH_BLOCK_IDS,
    SLASH_WRITING_IDS,
    type SlashBlockId,
  } from "$lib/utils/vaultMarkdownEdit";

  interface SlashItem {
    id: SlashBlockId;
    label: string;
    hint: string;
    keywords: string;
    group: "writing" | "blocks";
  }

  interface Props {
    open: boolean;
    filter?: string;
    anchor?: { top: number; left: number } | null;
    onSelect: (block: SlashBlockId) => void;
    onClose: () => void;
  }

  let { open, filter = "", anchor = null, onSelect, onClose }: Props = $props();

  const items: SlashItem[] = [
    { id: "wikilink", label: "Link to note", hint: "[[path|label]]", keywords: "link note wikilink wiki", group: "writing" },
    { id: "h1", label: "Title", hint: "# heading", keywords: "title h1 heading", group: "writing" },
    { id: "h2", label: "Section", hint: "## heading", keywords: "section h2 heading", group: "writing" },
    { id: "h3", label: "Subsection", hint: "### heading", keywords: "subsection h3 heading", group: "writing" },
    { id: "bullet", label: "Bullet list", hint: "- item", keywords: "bullet list ul", group: "writing" },
    { id: "numbered", label: "Numbered list", hint: "1. item", keywords: "numbered ordered ol", group: "writing" },
    { id: "checkbox", label: "To-do", hint: "- [ ] item", keywords: "todo task checkbox check", group: "writing" },
    { id: "link", label: "Web link", hint: "[text](url)", keywords: "link url href", group: "writing" },
    { id: "quote", label: "Quote", hint: "> quote", keywords: "quote blockquote", group: "writing" },
    { id: "callout", label: "Callout", hint: "> [!note]", keywords: "callout aside warning tip note obsidian", group: "writing" },
    { id: "divider", label: "Divider", hint: "---", keywords: "divider hr rule", group: "writing" },
    { id: "liquid_callout", label: "Liquid callout", hint: "```callout", keywords: "liquid callout aside tone note warn", group: "blocks" },
    { id: "liquid_card", label: "Card", hint: "```card", keywords: "liquid card summary", group: "blocks" },
    { id: "liquid_chart", label: "Chart", hint: "```chart", keywords: "liquid chart bar pie radar scatter combo heatmap plot graph", group: "blocks" },
    { id: "liquid_dashboard", label: "Dashboard", hint: "```dashboard", keywords: "liquid dashboard metrics pulse", group: "blocks" },
    { id: "liquid_report", label: "Report", hint: "```report", keywords: "liquid report narrative charts figures prose", group: "blocks" },
    { id: "liquid_tabs", label: "Tabs", hint: "```tabs", keywords: "liquid tabs panels switcher", group: "blocks" },
    { id: "liquid_steps", label: "Steps", hint: "```steps", keywords: "liquid steps numbered howto", group: "blocks" },
    { id: "liquid_accordion", label: "Accordion", hint: "```accordion", keywords: "liquid accordion collapse faq", group: "blocks" },
    { id: "liquid_code", label: "Code snippet", hint: "```code", keywords: "liquid code snippet copy diff", group: "blocks" },
    { id: "liquid_tree", label: "File tree", hint: "```tree", keywords: "liquid tree files folders", group: "blocks" },
    { id: "embed", label: "Embed note", hint: "![[note]]", keywords: "embed transclude include", group: "blocks" },
    { id: "toc", label: "Table of contents", hint: "medousa-toc", keywords: "toc contents table", group: "blocks" },
    { id: "view", label: "Query view", hint: "from a table", keywords: "view query table dashboard", group: "blocks" },
    { id: "board", label: "Kanban board", hint: "## columns", keywords: "board kanban columns", group: "blocks" },
    { id: "table", label: "Data table", hint: "| col |", keywords: "table database rows", group: "blocks" },
  ];

  const filteredItems = $derived.by(() => {
    const query = filter.trim().toLowerCase();
    const pool = items.filter((item) => {
      if (!query) return true;
      return (
        item.id.includes(query) ||
        item.label.toLowerCase().includes(query) ||
        item.hint.toLowerCase().includes(query) ||
        item.keywords.includes(query)
      );
    });
    const writing = pool.filter((item) => SLASH_WRITING_IDS.includes(item.id));
    const blocks = pool.filter((item) => SLASH_BLOCK_IDS.includes(item.id));
    return { writing, blocks, flat: [...writing, ...blocks] };
  });

  let highlightIndex = $state(0);
  let menuEl = $state<HTMLElement | null>(null);

  $effect(() => {
    if (open) highlightIndex = 0;
  });

  $effect(() => {
    filter;
    if (highlightIndex >= filteredItems.flat.length) {
      highlightIndex = Math.max(0, filteredItems.flat.length - 1);
    }
  });

  onMount(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (menuEl && target && menuEl.contains(target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  });

  export function handleMenuKeydown(event: KeyboardEvent): boolean {
    if (!open) return false;
    const visible = filteredItems.flat;
    if (visible.length === 0) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return true;
      }
      return false;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return true;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlightIndex = (highlightIndex + 1) % visible.length;
      return true;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      highlightIndex = (highlightIndex - 1 + visible.length) % visible.length;
      return true;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      onSelect(visible[highlightIndex]!.id);
      return true;
    }
    return false;
  }

  function flatIndexOf(id: SlashBlockId): number {
    return filteredItems.flat.findIndex((item) => item.id === id);
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="vault-slash-menu"
    class:vault-slash-menu--anchored={Boolean(anchor)}
    role="listbox"
    aria-label="Insert block"
    style:top={anchor ? `${anchor.top}px` : undefined}
    style:left={anchor ? `${anchor.left}px` : undefined}
  >
    <div class="vault-slash-menu-chrome">
      <p class="vault-slash-menu-title">Insert</p>
      <p class="vault-slash-menu-hint">↑↓ · Enter · Esc</p>
    </div>
    <ul class="vault-slash-menu-list">
      {#if filteredItems.writing.length > 0}
        <li class="vault-slash-menu-section" role="presentation">Writing</li>
        {#each filteredItems.writing as item (item.id)}
          {@const index = flatIndexOf(item.id)}
          <li>
            <button
              type="button"
              role="option"
              aria-selected={index === highlightIndex}
              class="vault-slash-menu-item"
              class:vault-slash-menu-item--active={index === highlightIndex}
              onclick={() => onSelect(item.id)}
            >
              <span>{item.label}</span>
              <span class="vault-slash-menu-item-hint">{item.hint}</span>
            </button>
          </li>
        {/each}
      {/if}
      {#if filteredItems.blocks.length > 0}
        <li class="vault-slash-menu-section" role="presentation">Blocks</li>
        {#each filteredItems.blocks as item (item.id)}
          {@const index = flatIndexOf(item.id)}
          <li>
            <button
              type="button"
              role="option"
              aria-selected={index === highlightIndex}
              class="vault-slash-menu-item"
              class:vault-slash-menu-item--active={index === highlightIndex}
              onclick={() => onSelect(item.id)}
            >
              <span>{item.label}</span>
              <span class="vault-slash-menu-item-hint">{item.hint}</span>
            </button>
          </li>
        {/each}
      {/if}
      {#if filteredItems.flat.length === 0}
        <li class="vault-slash-menu-empty">No matching blocks</li>
      {/if}
    </ul>
  </div>
{/if}
