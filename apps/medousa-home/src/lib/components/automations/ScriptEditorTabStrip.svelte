<script lang="ts">
  import { Plus, X } from "@lucide/svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  let renamingTabId = $state<string | null>(null);
  let renameDraft = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);

  function startRename(tabId: string, name: string, event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    renamingTabId = tabId;
    renameDraft = name;
    graphemeScriptEditor.selectTab(tabId);
    requestAnimationFrame(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  }

  function commitRename(tabId: string) {
    if (renamingTabId !== tabId) return;
    const trimmed = renameDraft.trim() || "Untitled script";
    graphemeScriptEditor.patchTab(tabId, { name: trimmed });
    renamingTabId = null;
  }

  function cancelRename() {
    renamingTabId = null;
  }
</script>

<div
  class="script-editor-tab-strip flex min-w-0 items-center gap-1 overflow-x-auto {compact
    ? 'flex-1'
    : ''}"
  role="tablist"
  aria-label="Open scripts"
>
  {#each graphemeScriptEditor.tabs as tab (tab.tabId)}
    <div
      class="script-editor-tab group flex max-w-[220px] shrink-0 items-center gap-1 rounded-t-md border border-b-0 px-2 py-1 text-[11px] {graphemeScriptEditor.activeTabId ===
      tab.tabId
        ? 'border-surface-500/60 bg-surface-900 text-primary-300'
        : 'border-transparent bg-transparent text-surface-400 hover:bg-surface-800/70'}"
      role="presentation"
    >
      {#if renamingTabId === tab.tabId}
        <input
          bind:this={renameInput}
          class="script-editor-tab-rename min-w-[5rem] max-w-[180px] bg-surface-950 px-1 py-0.5 text-[11px] text-surface-100 outline-none ring-1 ring-primary-500/50"
          type="text"
          bind:value={renameDraft}
          aria-label="Rename script tab"
          onblur={() => commitRename(tab.tabId)}
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitRename(tab.tabId);
            }
            if (event.key === "Escape") {
              event.preventDefault();
              cancelRename();
            }
          }}
        />
      {:else}
        <button
          type="button"
          role="tab"
          aria-selected={graphemeScriptEditor.activeTabId === tab.tabId}
          class="min-w-0 truncate"
          title="{tab.name} — double-click to rename"
          onclick={() => graphemeScriptEditor.selectTab(tab.tabId)}
          ondblclick={(event) => startRename(tab.tabId, tab.name, event)}
        >
          {tab.dirty ? "*" : ""}{tab.name}
        </button>
      {/if}
      {#if graphemeScriptEditor.tabs.length > 1 && renamingTabId !== tab.tabId}
        <button
          type="button"
          class="rounded p-0.5 text-surface-500 opacity-0 transition group-hover:opacity-100 hover:text-surface-200"
          aria-label="Close tab"
          onclick={() => graphemeScriptEditor.closeTab(tab.tabId)}
        >
          <X size={12} strokeWidth={2} />
        </button>
      {/if}
    </div>
  {/each}
  <button
    type="button"
    class="shrink-0 rounded-md p-1 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
    aria-label="New script tab"
    onclick={() => graphemeScriptEditor.openNewTab()}
  >
    <Plus size={14} strokeWidth={2} />
  </button>
</div>
