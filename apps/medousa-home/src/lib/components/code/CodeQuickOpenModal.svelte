<script lang="ts">
  /**
   * Quick Open modal: fuzzy file, @symbol, or :line.
   */
  import { FileCode2, ListTree, Search } from "@lucide/svelte";
  import { tick } from "svelte";
  import type { CodeQuickOpenController } from "$lib/code/codeQuickOpenController.svelte";

  interface Props {
    quick: CodeQuickOpenController;
    activeTitle?: string | null;
    pathFromUri: (uri?: string) => string | null;
  }

  let { quick, activeTitle = null, pathFromUri }: Props = $props();
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (!quick.open) return;
    void tick().then(() => input?.focus());
  });
</script>

{#if quick.open}
  <div class="fixed inset-0 z-[120] flex items-start justify-center px-4 pt-[12vh]">
    <button type="button" class="absolute inset-0 bg-black/35" aria-label="Close file picker" onclick={() => quick.close()}></button>
    <div class="relative w-full max-w-xl overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Open a file" tabindex="-1">
      <div class="flex items-center gap-2 border-b border-surface-500/30 px-3">
        <Search size={14} class="text-content-quiet" />
        <input
          bind:this={input}
          class="min-w-0 flex-1 bg-transparent py-2.5 text-sm text-surface-100 outline-none"
          placeholder="Fuzzy file, @symbol, or :line"
          bind:value={quick.query}
          oninput={() => quick.onQueryInput()}
          onkeydown={(event) => {
            if (event.key === "ArrowDown") { event.preventDefault(); quick.moveIndex(1); }
            if (event.key === "ArrowUp") { event.preventDefault(); quick.moveIndex(-1); }
            if (event.key === "Enter") { event.preventDefault(); quick.chooseResult(); }
          }}
        />
        <span class="text-chrome-xs text-content-faint">⌘P</span>
      </div>
      <div class="max-h-[50vh] overflow-y-auto py-1">
        {#if quick.mode === "line"}
          <button type="button" class="flex w-full items-center gap-2 px-3 py-2 text-left text-content-secondary hover:bg-surface-800" onclick={() => quick.chooseLine()}>
            <span class="font-mono text-xs text-content-link">:{quick.query.slice(1).trim() || "line"}</span>
            <span class="text-chrome-sm text-content-quiet">Go to a line in {activeTitle}</span>
          </button>
        {:else if quick.mode === "symbol" && quick.symbolResults.length === 0}
          <p class="px-3 py-3 text-xs text-content-quiet">No matching project symbols.</p>
        {:else if quick.mode === "symbol"}
          {#each quick.symbolResults as symbol, index (`${symbol.name}:${symbol.location?.uri}:${symbol.location?.range?.start?.line}`)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quick.index ? 'bg-surface-800 text-surface-100' : 'text-content-tertiary hover:bg-surface-900'}" onmouseenter={() => (quick.index = index)} onclick={() => void quick.chooseSymbol(symbol)}>
              <ListTree size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{symbol.name}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-chrome-xs text-content-faint">{symbol.containerName ?? pathFromUri(symbol.location?.uri) ?? ""}</span>
            </button>
          {/each}
        {:else if quick.loading}
          <p class="px-3 py-3 text-xs text-content-quiet">Reading project files…</p>
        {:else if quick.fileResults.length === 0}
          <p class="px-3 py-3 text-xs text-content-quiet">No matching files.</p>
        {:else}
          {#each quick.fileResults as file, index (file.path)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quick.index ? 'bg-surface-800 text-surface-100' : 'text-content-tertiary hover:bg-surface-900'}" onmouseenter={() => (quick.index = index)} onclick={() => void quick.chooseFile(file)}>
              <FileCode2 size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{file.path.split("/").pop()}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-chrome-xs text-content-faint">{file.path}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}
