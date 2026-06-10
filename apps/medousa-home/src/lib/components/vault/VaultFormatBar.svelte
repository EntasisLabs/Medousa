<script lang="ts">
  import {
    Bold,
    Italic,
    Heading1,
    Heading2,
    Heading3,
    List,
    ListOrdered,
    Link,
    Code,
    SquareCheck,
    Highlighter,
  } from "@lucide/svelte";
  import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
  import {
    MARKDOWN_COLOR_OPTIONS,
    type MarkdownColorId,
  } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    disabled?: boolean;
    onFormat: (action: MarkdownFormatAction) => void;
    onColor: (color: MarkdownColorId) => void;
  }

  let { disabled = false, onFormat, onColor }: Props = $props();

  const groups: {
    label: string;
    items: { action: MarkdownFormatAction; title: string; Icon: typeof Bold }[];
  }[] = [
    {
      label: "Style",
      items: [
        { action: "bold", title: "Bold", Icon: Bold },
        { action: "italic", title: "Italic", Icon: Italic },
        { action: "code", title: "Inline code", Icon: Code },
        { action: "highlight", title: "Highlight", Icon: Highlighter },
      ],
    },
    {
      label: "Structure",
      items: [
        { action: "h1", title: "Title", Icon: Heading1 },
        { action: "h2", title: "Section", Icon: Heading2 },
        { action: "h3", title: "Subsection", Icon: Heading3 },
      ],
    },
    {
      label: "Lists",
      items: [
        { action: "bullet", title: "Bullet list", Icon: List },
        { action: "numbered", title: "Numbered list", Icon: ListOrdered },
        { action: "checkbox", title: "Checkbox", Icon: SquareCheck },
      ],
    },
    {
      label: "Insert",
      items: [{ action: "link", title: "Link", Icon: Link }],
    },
  ];
</script>

<div
  class="vault-format-bar flex flex-wrap items-center gap-1 border-b border-surface-500/40 bg-surface-900/80 px-3 py-1.5"
  role="toolbar"
  aria-label="Formatting"
>
  {#each groups as group, groupIndex (group.label)}
    {#if groupIndex > 0}
      <span class="mx-0.5 h-5 w-px bg-surface-600/80" aria-hidden="true"></span>
    {/if}
    {#each group.items as item (item.action)}
      <button
        type="button"
        class="vault-format-btn"
        title={item.title}
        aria-label={item.title}
        {disabled}
        onclick={() => onFormat(item.action)}
      >
        <item.Icon size={15} strokeWidth={2} />
      </button>
    {/each}
  {/each}

  <span class="mx-0.5 h-5 w-px bg-surface-600/80" aria-hidden="true"></span>

  <div class="flex items-center gap-1" role="group" aria-label="Text color">
    {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
      <button
        type="button"
        class="vault-color-swatch"
        title={color.label}
        aria-label={`Color: ${color.label}`}
        style:background-color={color.swatch}
        {disabled}
        onclick={() => onColor(color.id)}
      ></button>
    {/each}
  </div>

  <p class="ml-auto hidden text-[11px] text-surface-500 sm:block">
    Select text to format · type <kbd class="vault-kbd">/</kbd> for blocks
  </p>
</div>
