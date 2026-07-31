<script lang="ts">
  /**
   * Side outline of document symbols (from LSP when available).
   * Parent supplies symbols; this is chrome only.
   */
  interface OutlineSymbol {
    name: string;
    kind: string;
    line: number;
  }

  interface Props {
    symbols?: OutlineSymbol[];
    onSelect?: (line: number) => void;
    onClose?: () => void;
  }

  let { symbols = [], onSelect, onClose }: Props = $props();
</script>

<aside
  class="code-editor-outline flex h-full min-h-0 w-48 shrink-0 flex-col border-l border-surface-500/40 bg-surface-900/40"
  aria-label="Document outline"
>
  <div
    class="flex items-center justify-between border-b border-surface-500/30 px-2 py-1.5 text-[11px] font-medium uppercase tracking-wide text-surface-300"
  >
    <span>Outline</span>
    {#if onClose}
      <button
        type="button"
        class="rounded px-1 text-surface-400 hover:bg-surface-700/60 hover:text-surface-100"
        onclick={() => onClose?.()}
      >
        Hide
      </button>
    {/if}
  </div>
  <ul class="min-h-0 flex-1 overflow-y-auto px-1 py-1 text-[12px]">
    {#if symbols.length === 0}
      <li class="px-2 py-2 text-surface-400">No symbols</li>
    {:else}
      {#each symbols as sym (sym.name + sym.line)}
        <li>
          <button
            type="button"
            class="flex w-full items-baseline gap-1 rounded px-2 py-0.5 text-left hover:bg-surface-700/50"
            onclick={() => onSelect?.(sym.line)}
          >
            <span class="shrink-0 text-[10px] uppercase text-surface-500">{sym.kind}</span>
            <span class="min-w-0 truncate text-surface-100">{sym.name}</span>
            <span class="ml-auto shrink-0 text-[10px] text-surface-500">{sym.line}</span>
          </button>
        </li>
      {/each}
    {/if}
  </ul>
</aside>
