<script lang="ts">
  import {
    Bot,
    CalendarClock,
    FileCode2,
    FileText,
    Files,
    GitBranch,
    Presentation,
    X,
  } from "@lucide/svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { persistScriptName } from "$lib/grapheme/scriptWorkbenchActions";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";

  let renamingTabId = $state<string | null>(null);
  let renameDraft = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);
  let longPressTimer: ReturnType<typeof setTimeout> | null = null;
  let renameBusy = $state(false);
  let handledRenameToken = $state(-1);

  const mobile = $derived(layout.isMobile);

  function startRename(tabId: string, title: string, event?: Event) {
    event?.preventDefault();
    event?.stopPropagation();
    const tab = lmeWorkspace.tabs.find((entry) => entry.tabId === tabId);
    if (!tab || tab.kind !== "script") return;
    renamingTabId = tabId;
    renameDraft = title;
    void lmeWorkspace.activateTab(tabId);
    requestAnimationFrame(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  }

  function scheduleLongPressRename(tabId: string, title: string) {
    clearLongPress();
    longPressTimer = setTimeout(() => startRename(tabId, title), 480);
  }

  function clearLongPress() {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  async function commitRename(tabId: string) {
    if (renamingTabId !== tabId || renameBusy) return;
    const lmeTab = lmeWorkspace.tabs.find((entry) => entry.tabId === tabId);
    if (!lmeTab || lmeTab.kind !== "script") {
      renamingTabId = null;
      scriptRenameUi.clearLme();
      return;
    }
    const editorTab = graphemeScriptEditor.tabs.find(
      (entry) => entry.tabId === lmeTab.scriptTabId,
    );
    if (!editorTab) {
      renamingTabId = null;
      scriptRenameUi.clearLme();
      return;
    }
    const trimmed = renameDraft.trim() || "Untitled script";
    renameBusy = true;
    try {
      await persistScriptName(editorTab, trimmed);
    } catch {
      graphemeScriptEditor.patchTab(editorTab.tabId, { name: trimmed });
      lmeWorkspace.syncScriptTabFromEditor({ activate: false });
    } finally {
      renameBusy = false;
      renamingTabId = null;
      scriptRenameUi.clearLme();
    }
  }

  function cancelRename() {
    renamingTabId = null;
    scriptRenameUi.clearLme();
  }

  $effect(() => {
    const tabId = scriptRenameUi.lmeTabId;
    const token = scriptRenameUi.token;
    if (!tabId || token === handledRenameToken) return;
    handledRenameToken = token;
    const tab = lmeWorkspace.tabs.find((entry) => entry.tabId === tabId);
    if (!tab || tab.kind !== "script") {
      scriptRenameUi.clearLme();
      return;
    }
    startRename(tab.tabId, tab.title);
  });
</script>

{#if !layout.shellSidebarExpanded || lmeWorkspace.tabs.length > 0}
  <div
    class="lme-tab-strip flex min-w-0 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-surface-500/40 bg-surface-950/60 px-1.5 pt-1"
    role="tablist"
    aria-label="Open workspace documents"
  >
    {#if !layout.shellSidebarExpanded}
      <div class="mb-0.5">
        <ShellSidebarExpandButton label="Show workspace browser" />
      </div>
    {/if}
    {#each lmeWorkspace.tabs as tab (tab.tabId)}
      {@const active = lmeWorkspace.activeTabId === tab.tabId}
      <div
        class="group flex max-w-[200px] shrink-0 items-center gap-1 rounded-t-md border border-b-0 px-2 py-1 text-[11px]
          {active
          ? 'border-surface-500/55 bg-surface-900 text-primary-300'
          : 'border-transparent text-surface-400 hover:bg-surface-800/70'}"
        role="presentation"
      >
        {#if renamingTabId === tab.tabId && tab.kind === "script"}
          <div class="flex min-w-0 flex-1 items-center gap-1">
            <FileCode2 size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
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
          </div>
        {:else}
          <button
            type="button"
            role="tab"
            aria-selected={active}
            class="flex min-w-0 flex-1 items-center gap-1 text-left"
            title={tab.kind === "script"
              ? mobile
                ? `${tab.title} — long-press to rename`
                : `${tab.title} — double-click to rename`
              : tab.title}
            onclick={() => void lmeWorkspace.activateTab(tab.tabId)}
            ondblclick={tab.kind === "script" && !mobile
              ? (event) => startRename(tab.tabId, tab.title, event)
              : undefined}
            onpointerdown={tab.kind === "script" && mobile
              ? () => scheduleLongPressRename(tab.tabId, tab.title)
              : undefined}
            onpointerup={tab.kind === "script" && mobile ? clearLongPress : undefined}
            onpointerleave={tab.kind === "script" && mobile ? clearLongPress : undefined}
            onpointercancel={tab.kind === "script" && mobile ? clearLongPress : undefined}
          >
            {#if tab.kind === "script"}
              <FileCode2 size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else if tab.kind === "file"}
              <Files size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else if tab.kind === "deck"}
              <Presentation size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else if tab.kind === "manuscript"}
              <Bot size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else if tab.kind === "flow"}
              <GitBranch size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else if tab.kind === "schedule"}
              <CalendarClock size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {:else}
              <FileText size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
            {/if}
            <span class="truncate">{tab.title}</span>
          </button>
        {/if}
        {#if renamingTabId !== tab.tabId}
          <button
            type="button"
            class="rounded p-0.5 opacity-0 transition-opacity hover:bg-surface-700 group-hover:opacity-100 focus:opacity-100"
            aria-label="Close {tab.title}"
            onclick={() => void lmeWorkspace.closeTab(tab.tabId)}
          >
            <X size={11} strokeWidth={2} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}
