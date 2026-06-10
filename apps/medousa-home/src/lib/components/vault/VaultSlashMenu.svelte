<script lang="ts">
  import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";

  interface SlashItem {
    id: SlashBlockId;
    label: string;
    hint: string;
  }

  interface Props {
    open: boolean;
    onSelect: (block: SlashBlockId) => void;
    onClose: () => void;
  }

  let { open, onSelect, onClose }: Props = $props();

  const items: SlashItem[] = [
    { id: "h1", label: "Title", hint: "Large heading" },
    { id: "h2", label: "Section", hint: "Section heading" },
    { id: "h3", label: "Subsection", hint: "Smaller heading" },
    { id: "bullet", label: "Bullet list", hint: "- item" },
    { id: "numbered", label: "Numbered list", hint: "1. item" },
    { id: "checkbox", label: "To-do", hint: "- [ ] item" },
    { id: "link", label: "Link", hint: "[text](url)" },
    { id: "quote", label: "Quote", hint: "> quote" },
    { id: "divider", label: "Divider", hint: "---" },
  ];

  let highlightIndex = $state(0);

  $effect(() => {
    if (open) highlightIndex = 0;
  });

  export function handleMenuKeydown(event: KeyboardEvent): boolean {
    if (!open) return false;
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return true;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlightIndex = (highlightIndex + 1) % items.length;
      return true;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      highlightIndex = (highlightIndex - 1 + items.length) % items.length;
      return true;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      onSelect(items[highlightIndex]!.id);
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
      {#each items as item, index (item.id)}
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
      {/each}
    </ul>
  </div>
{/if}
