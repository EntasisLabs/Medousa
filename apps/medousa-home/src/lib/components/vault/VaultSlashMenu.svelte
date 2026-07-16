<script lang="ts">
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
    {
      id: "wikilink",
      label: "Link to note",
      hint: "Jump to a note",
      keywords: "link note wikilink wiki [[",
      group: "writing",
    },
    {
      id: "h1",
      label: "Title",
      hint: "Big heading",
      keywords: "title h1 heading #",
      group: "writing",
    },
    {
      id: "h2",
      label: "Section",
      hint: "Section heading",
      keywords: "section h2 heading ##",
      group: "writing",
    },
    {
      id: "h3",
      label: "Subsection",
      hint: "Smaller heading",
      keywords: "subsection h3 heading ###",
      group: "writing",
    },
    {
      id: "bullet",
      label: "Bullet list",
      hint: "Unordered list",
      keywords: "bullet list ul -",
      group: "writing",
    },
    {
      id: "numbered",
      label: "Numbered list",
      hint: "Ordered list",
      keywords: "numbered ordered ol 1.",
      group: "writing",
    },
    {
      id: "checkbox",
      label: "To-do",
      hint: "Checklist item",
      keywords: "todo task checkbox check [ ]",
      group: "writing",
    },
    {
      id: "link",
      label: "Web link",
      hint: "Link to a URL",
      keywords: "link url href http",
      group: "writing",
    },
    {
      id: "quote",
      label: "Quote",
      hint: "Quoted passage",
      keywords: "quote blockquote >",
      group: "writing",
    },
    {
      id: "divider",
      label: "Divider",
      hint: "Horizontal rule",
      keywords: "divider hr rule ---",
      group: "writing",
    },
    {
      id: "liquid_callout",
      label: "Callout",
      hint: "Highlighted aside",
      keywords: "liquid callout aside tone note warn tip warning important ```callout > [!note] obsidian",
      group: "blocks",
    },
    {
      id: "liquid_card",
      label: "Card",
      hint: "Summary card",
      keywords: "liquid card summary ```card",
      group: "blocks",
    },
    {
      id: "liquid_chart",
      label: "Chart",
      hint: "Bar, line, pie…",
      keywords: "liquid chart bar pie radar scatter combo heatmap plot graph ```chart",
      group: "blocks",
    },
    {
      id: "liquid_dashboard",
      label: "Dashboard",
      hint: "Metrics at a glance",
      keywords: "liquid dashboard metrics pulse ```dashboard",
      group: "blocks",
    },
    {
      id: "liquid_report",
      label: "Report",
      hint: "Narrative with figures",
      keywords: "liquid report narrative charts figures prose ```report",
      group: "blocks",
    },
    {
      id: "liquid_tabs",
      label: "Tabs",
      hint: "Switchable panels",
      keywords: "liquid tabs panels switcher ```tabs",
      group: "blocks",
    },
    {
      id: "liquid_steps",
      label: "Steps",
      hint: "Numbered how-to",
      keywords: "liquid steps numbered howto ```steps",
      group: "blocks",
    },
    {
      id: "liquid_accordion",
      label: "Accordion",
      hint: "Expandable sections",
      keywords: "liquid accordion collapse faq ```accordion",
      group: "blocks",
    },
    {
      id: "liquid_code",
      label: "Code snippet",
      hint: "Copyable code",
      keywords: "liquid code snippet copy diff ```code",
      group: "blocks",
    },
    {
      id: "liquid_tree",
      label: "File tree",
      hint: "Folders and files",
      keywords: "liquid tree files folders ```tree",
      group: "blocks",
    },
    {
      id: "embed",
      label: "Embed note",
      hint: "Show another note here",
      keywords: "embed transclude include ![[",
      group: "blocks",
    },
    {
      id: "toc",
      label: "Table of contents",
      hint: "Jump links for headings",
      keywords: "toc contents table medousa-toc",
      group: "blocks",
    },
    {
      id: "view",
      label: "Query view",
      hint: "Live table from notes",
      keywords: "view query table dashboard",
      group: "blocks",
    },
    {
      id: "board",
      label: "Kanban board",
      hint: "Cards in columns",
      keywords: "board kanban columns ##",
      group: "blocks",
    },
    {
      id: "table",
      label: "Data table",
      hint: "Rows and columns",
      keywords: "table database rows |",
      group: "blocks",
    },
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
  let listEl = $state<HTMLElement | null>(null);
  let wasOpen = false;

  $effect(() => {
    if (open && !wasOpen) {
      highlightIndex = 0;
    }
    wasOpen = open;
  });

  $effect(() => {
    filter;
    if (highlightIndex >= filteredItems.flat.length) {
      highlightIndex = Math.max(0, filteredItems.flat.length - 1);
    }
  });

  $effect(() => {
    if (!open || !listEl) return;
    const index = highlightIndex;
    const active = listEl.querySelector<HTMLElement>(
      `[data-slash-index="${index}"]`,
    );
    if (!active) return;
    const listRect = listEl.getBoundingClientRect();
    const rowRect = active.getBoundingClientRect();
    if (rowRect.top < listRect.top) {
      listEl.scrollTop -= listRect.top - rowRect.top + 4;
    } else if (rowRect.bottom > listRect.bottom) {
      listEl.scrollTop += rowRect.bottom - listRect.bottom + 4;
    }
  });

  // Click-outside only — keyboard nav is owned by CodeMirror Prec.highest keymap.
  $effect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (menuEl && target && menuEl.contains(target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  });

  /** Called from the editor keymap (single owner of ↑↓/Enter/Esc). */
  export function handleMenuKey(key: string): boolean {
    if (!open) return false;
    const visible = filteredItems.flat;
    if (visible.length === 0) {
      if (key === "Escape") {
        onClose();
        return true;
      }
      return false;
    }
    if (key === "Escape") {
      onClose();
      return true;
    }
    if (key === "ArrowDown") {
      highlightIndex = (highlightIndex + 1) % visible.length;
      return true;
    }
    if (key === "ArrowUp") {
      highlightIndex = (highlightIndex - 1 + visible.length) % visible.length;
      return true;
    }
    if (key === "Enter") {
      onSelect(visible[highlightIndex]!.id);
      return true;
    }
    return false;
  }

  function flatIndexOf(id: SlashBlockId): number {
    return filteredItems.flat.findIndex((item) => item.id === id);
  }

  function selectItem(id: SlashBlockId, event: Event) {
    event.preventDefault();
    onSelect(id);
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
    <ul bind:this={listEl} class="vault-slash-menu-list">
      {#if filteredItems.writing.length > 0}
        <li class="vault-slash-menu-section" role="presentation">Writing</li>
        {#each filteredItems.writing as item (item.id)}
          {@const index = flatIndexOf(item.id)}
          <li role="presentation">
            <div
              role="option"
              data-slash-index={index}
              aria-selected={index === highlightIndex}
              class="vault-slash-menu-item"
              class:vault-slash-menu-item--active={index === highlightIndex}
              onpointerdown={(event) => selectItem(item.id, event)}
            >
              <span>{item.label}</span>
              <span class="vault-slash-menu-item-hint">{item.hint}</span>
            </div>
          </li>
        {/each}
      {/if}
      {#if filteredItems.blocks.length > 0}
        <li class="vault-slash-menu-section" role="presentation">Blocks</li>
        {#each filteredItems.blocks as item (item.id)}
          {@const index = flatIndexOf(item.id)}
          <li role="presentation">
            <div
              role="option"
              data-slash-index={index}
              aria-selected={index === highlightIndex}
              class="vault-slash-menu-item"
              class:vault-slash-menu-item--active={index === highlightIndex}
              onpointerdown={(event) => selectItem(item.id, event)}
            >
              <span>{item.label}</span>
              <span class="vault-slash-menu-item-hint">{item.hint}</span>
            </div>
          </li>
        {/each}
      {/if}
      {#if filteredItems.flat.length === 0}
        <li class="vault-slash-menu-empty">No matching blocks</li>
      {/if}
    </ul>
  </div>
{/if}
