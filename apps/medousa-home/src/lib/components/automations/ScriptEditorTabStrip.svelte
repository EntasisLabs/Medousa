<script lang="ts">
  import { Plus, X } from "@lucide/svelte";
  import { persistScriptName } from "$lib/grapheme/scriptWorkbenchActions";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";

  interface Props {
    compact?: boolean;
    mobile?: boolean;
  }

  let { compact = false, mobile = false }: Props = $props();

  let renamingTabId = $state<string | null>(null);
  let renameDraft = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);
  let longPressTimer: ReturnType<typeof setTimeout> | null = null;
  let renameBusy = $state(false);
  let handledRenameToken = $state(-1);

  function startRename(tabId: string, name: string, event?: Event) {
    event?.preventDefault();
    event?.stopPropagation();
    renamingTabId = tabId;
    renameDraft = name;
    graphemeScriptEditor.selectTab(tabId);
    requestAnimationFrame(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  }

  function scheduleLongPressRename(tabId: string, name: string) {
    clearLongPress();
    longPressTimer = setTimeout(() => startRename(tabId, name), 480);
  }

  function clearLongPress() {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  async function commitRename(tabId: string) {
    if (renamingTabId !== tabId || renameBusy) return;
    const tab = graphemeScriptEditor.tabs.find((entry) => entry.tabId === tabId);
    if (!tab) {
      renamingTabId = null;
      scriptRenameUi.clearEditor();
      return;
    }
    const trimmed = renameDraft.trim() || "Untitled script";
    renameBusy = true;
    try {
      await persistScriptName(tab, trimmed);
    } catch {
      graphemeScriptEditor.patchTab(tabId, { name: trimmed });
    } finally {
      renameBusy = false;
      renamingTabId = null;
      scriptRenameUi.clearEditor();
    }
  }

  function cancelRename() {
    renamingTabId = null;
    scriptRenameUi.clearEditor();
  }

  $effect(() => {
    const tabId = scriptRenameUi.editorTabId;
    const token = scriptRenameUi.token;
    if (!tabId || token === handledRenameToken) return;
    handledRenameToken = token;
    const tab = graphemeScriptEditor.tabs.find((entry) => entry.tabId === tabId);
    if (!tab) {
      scriptRenameUi.clearEditor();
      return;
    }
    startRename(tab.tabId, tab.name);
  });
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
          class="script-editor-tab-rename"
          type="text"
          bind:value={renameDraft}
          aria-label="Rename script tab"
          spellcheck="false"
          onblur={() => void commitRename(tab.tabId)}
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void commitRename(tab.tabId);
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
          title={mobile ? `${tab.name} — long-press to rename` : `${tab.name} — double-click to rename`}
          onclick={() => graphemeScriptEditor.selectTab(tab.tabId)}
          ondblclick={mobile ? undefined : (event) => startRename(tab.tabId, tab.name, event)}
          onpointerdown={mobile
            ? () => scheduleLongPressRename(tab.tabId, tab.name)
            : undefined}
          onpointerup={mobile ? clearLongPress : undefined}
          onpointerleave={mobile ? clearLongPress : undefined}
          onpointercancel={mobile ? clearLongPress : undefined}
        >
          {tab.dirty ? "*" : ""}{tab.name}
        </button>
      {/if}
      {#if renamingTabId !== tab.tabId}
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
