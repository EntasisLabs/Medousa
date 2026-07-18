<script lang="ts">
  import { FileCode2, FileText, Files, Presentation, X } from "@lucide/svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
</script>

{#if lmeWorkspace.tabs.length > 0}
  <div
    class="lme-tab-strip flex min-w-0 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-surface-500/40 bg-surface-950/60 px-1.5 pt-1"
    role="tablist"
    aria-label="Open workspace documents"
  >
    {#each lmeWorkspace.tabs as tab (tab.tabId)}
      {@const active = lmeWorkspace.activeTabId === tab.tabId}
      <div
        class="group flex max-w-[200px] shrink-0 items-center gap-1 rounded-t-md border border-b-0 px-2 py-1 text-[11px]
          {active
          ? 'border-surface-500/55 bg-surface-900 text-primary-300'
          : 'border-transparent text-surface-400 hover:bg-surface-800/70'}"
        role="presentation"
      >
        <button
          type="button"
          role="tab"
          aria-selected={active}
          class="flex min-w-0 flex-1 items-center gap-1 text-left"
          onclick={() => void lmeWorkspace.activateTab(tab.tabId)}
        >
          {#if tab.kind === "script"}
            <FileCode2 size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
          {:else if tab.kind === "file"}
            <Files size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
          {:else if tab.kind === "deck"}
            <Presentation size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
          {:else}
            <FileText size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
          {/if}
          <span class="truncate">{tab.title}</span>
        </button>
        <button
          type="button"
          class="rounded p-0.5 opacity-0 transition-opacity hover:bg-surface-700 group-hover:opacity-100 focus:opacity-100"
          aria-label="Close {tab.title}"
          onclick={() => void lmeWorkspace.closeTab(tab.tabId)}
        >
          <X size={11} strokeWidth={2} />
        </button>
      </div>
    {/each}
  </div>
{/if}
