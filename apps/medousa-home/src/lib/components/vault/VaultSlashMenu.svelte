<script lang="ts">
  import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";

  interface SlashItem {
    id: SlashBlockId;
    label: string;
    hint: string;
    keywords: string;
  }

  interface Props {
    open: boolean;
    filter?: string;
    onSelect: (block: SlashBlockId) => void;
    onClose: () => void;
  }

  let { open, filter = "", onSelect, onClose }: Props = $props();

  const items: SlashItem[] = [
    { id: "wikilink", label: "Link to note", hint: "[[path|label]]", keywords: "link note wikilink wiki" },
    { id: "h1", label: "Title", hint: "# heading", keywords: "title h1 heading" },
    { id: "h2", label: "Section", hint: "## heading", keywords: "section h2 heading" },
    { id: "h3", label: "Subsection", hint: "### heading", keywords: "subsection h3 heading" },
    { id: "bullet", label: "Bullet list", hint: "- item", keywords: "bullet list ul" },
    { id: "numbered", label: "Numbered list", hint: "1. item", keywords: "numbered ordered ol" },
    { id: "checkbox", label: "To-do", hint: "- [ ] item", keywords: "todo task checkbox check" },
    { id: "link", label: "Web link", hint: "[text](url)", keywords: "link url href" },
    { id: "quote", label: "Quote", hint: "> quote", keywords: "quote blockquote" },
    { id: "callout", label: "Callout", hint: "> [!note]", keywords: "callout aside warning tip note obsidian" },
    { id: "liquid_callout", label: "Liquid callout", hint: "```callout", keywords: "liquid callout aside tone note warn" },
    { id: "liquid_card", label: "Card", hint: "```card", keywords: "liquid card summary" },
    { id: "liquid_chart", label: "Chart", hint: "```chart", keywords: "liquid chart bar pie radar plot graph" },
    { id: "liquid_dashboard", label: "Dashboard", hint: "```dashboard", keywords: "liquid dashboard metrics pulse" },
    { id: "embed", label: "Embed note", hint: "![[note]]", keywords: "embed transclude include" },
    { id: "divider", label: "Divider", hint: "---", keywords: "divider hr rule" },
    { id: "toc", label: "Table of contents", hint: "medousa-toc", keywords: "toc contents table" },
    { id: "view", label: "Query view", hint: "from a table", keywords: "view query table dashboard" },
    { id: "board", label: "Kanban board", hint: "## columns", keywords: "board kanban columns" },
    { id: "table", label: "Data table", hint: "| col |", keywords: "table database rows" },
  ];

  const filteredItems = $derived.by(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return items;
    return items.filter(
      (item) =>
        item.id.includes(query) ||
        item.label.toLowerCase().includes(query) ||
        item.hint.toLowerCase().includes(query) ||
        item.keywords.includes(query),
    );
  });

  let highlightIndex = $state(0);

  $effect(() => {
    if (open) highlightIndex = 0;
  });

  $effect(() => {
    filter;
    if (highlightIndex >= filteredItems.length) {
      highlightIndex = Math.max(0, filteredItems.length - 1);
    }
  });

  export function handleMenuKeydown(event: KeyboardEvent): boolean {
    if (!open) return false;
    const visible = filteredItems;
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
</script>

{#if open}
  <div
    class="vault-slash-menu shrink-0 border-b border-primary-500/30 bg-surface-900/95 shadow-lg"
    role="listbox"
    aria-label="Insert block"
  >
    <div class="flex items-center justify-between gap-2 px-3 py-1.5">
      <p class="text-[10px] font-medium uppercase tracking-wide text-primary-300">
        Insert block
      </p>
      <p class="text-[10px] text-surface-500">
        ↑↓ · Enter · Esc
      </p>
    </div>
    <ul class="grid max-h-48 grid-cols-1 gap-0.5 overflow-y-auto px-2 pb-2 sm:grid-cols-2">
      {#each filteredItems as item, index (item.id)}
        <li>
          <button
            type="button"
            role="option"
            aria-selected={index === highlightIndex}
            class="flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors {index ===
            highlightIndex
              ? 'bg-primary-500/20 text-primary-100'
              : 'text-surface-200 hover:bg-surface-800'}"
            onclick={() => onSelect(item.id)}
          >
            <span>{item.label}</span>
            <span class="font-mono text-[10px] text-surface-500">{item.hint}</span>
          </button>
        </li>
      {:else}
        <li class="px-2.5 py-3 text-sm text-surface-500">No matching blocks</li>
      {/each}
    </ul>
  </div>
{/if}
